//! Embedding boundary for scoped WASM Service egress.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use actr_pack::egress::{EgressPermissions, HttpsPermission, PackagePrivacyContract};
use actr_protocol::{ActorResult, ActrError, ActrId, PayloadType};
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::request_scope::RequestScope;
use crate::workload::{HostAbiFn, HostOperation, HostOperationResult, InvocationContext};

const MAX_REQUEST: usize = 64 * 1024;
const MAX_RESPONSE: usize = 256 * 1024;
const MAX_DURATION: Duration = Duration::from_secs(5);

/// A host-owned request. All invocation identities come from the runtime.
pub struct EgressRequest<'a> {
    pub invocation: &'a InvocationContext,
    pub source_package_sha256: &'a str,
    pub privacy_contract_sha256: &'a str,
    /// Signed upper bound on the data classes for this operation.
    pub data_classes: &'a [String],
    pub alias: &'a str,
    pub operation: EgressOperation<'a>,
}

/// Operations that the embedding Host must authorize against current policy.
pub enum EgressOperation<'a> {
    Actor {
        route: &'a str,
        target_package_sha256: &'a str,
        target_actor_type: &'a str,
        contract_fingerprint: &'a str,
    },
    Https {
        url: &'a str,
        method: &'a str,
        required: bool,
    },
}

/// A short-lived decision from an embedding Host. The Host must cancel the
/// token when the underlying grant, security state, or applicable lease changes.
pub struct EgressPermit {
    pub expires_at: Instant,
    pub revocation: CancellationToken,
    pub target: Option<ActrId>,
}

/// The trusted application Host implements this using current instance grants,
/// verified peer facts and data provenance. Returning a permit asserts those
/// checks have passed for this operation and invocation.
pub trait EgressPolicy: Send + Sync {
    fn authorize(&self, request: EgressRequest<'_>) -> ActorResult<EgressPermit>;
    /// Check authenticated inbound caller, route and current source state.
    fn authorize_inbound(
        &self,
        invocation: &InvocationContext,
        source_package_sha256: &str,
        privacy_contract_sha256: &str,
        route: &str,
    ) -> ActorResult<()>;
}

pub(crate) struct EgressBroker {
    policy: Arc<dyn EgressPolicy>,
    digest: String,
    contract_sha256: String,
    permissions: EgressPermissions,
    locks: BTreeMap<String, actr_config::lock::LockedDependency>,
    concurrency: Semaphore,
    #[cfg(test)]
    https_fixture: Option<HttpsFixture>,
}

impl EgressBroker {
    pub(crate) fn new(
        policy: Arc<dyn EgressPolicy>,
        digest: String,
        contract: PackagePrivacyContract,
        lock: Option<&actr_config::lock::LockFile>,
    ) -> Self {
        let locks = lock
            .into_iter()
            .flat_map(|lock| &lock.dependencies)
            .map(|entry| (entry.alias.clone(), entry.clone()))
            .collect();
        Self {
            policy,
            digest,
            contract_sha256: contract.sha256,
            permissions: contract.egress,
            locks,
            concurrency: Semaphore::new(8),
            #[cfg(test)]
            https_fixture: None,
        }
    }

    pub(crate) fn authorize_inbound(
        &self,
        invocation: &InvocationContext,
        route: &str,
    ) -> ActorResult<()> {
        self.policy
            .authorize_inbound(invocation, &self.digest, &self.contract_sha256, route)
    }

    pub(crate) async fn actor_call(
        &self,
        invocation: &InvocationContext,
        bridge: HostAbiFn,
        alias: &str,
        route: String,
        payload: Vec<u8>,
        scope: Option<&RequestScope>,
    ) -> ActorResult<Vec<u8>> {
        use actr_framework::guest::dynclib_abi::HostCallRawV1;
        if scope.is_some_and(|scope| !scope.is_active()) {
            return Err(ActrError::TimedOut);
        }
        let _slot = self
            .concurrency
            .try_acquire()
            .map_err(|_| deny("egress concurrency limit"))?;
        let permission = self
            .permissions
            .actor
            .get(alias)
            .filter(|scope| scope.rpc_routes.contains(&route))
            .ok_or_else(|| deny("Actor route is outside the signed scope"))?;
        if payload.len() > MAX_REQUEST {
            return Err(deny("Actor payload exceeds its limit"));
        }
        let lock = self
            .locks
            .get(alias)
            .ok_or_else(|| deny("missing dependency lock"))?;
        let digest = lock
            .package_sha256
            .as_deref()
            .ok_or_else(|| deny("missing dependency digest"))?;
        let permit = self.policy.authorize(EgressRequest {
            invocation,
            source_package_sha256: &self.digest,
            privacy_contract_sha256: &self.contract_sha256,
            data_classes: &permission.data_classes,
            alias,
            operation: EgressOperation::Actor {
                route: &route,
                target_package_sha256: digest,
                target_actor_type: &lock.actr_type,
                contract_fingerprint: &lock.fingerprint,
            },
        })?;
        let target = permit
            .target
            .clone()
            .filter(|target| target.realm == invocation.self_id.realm)
            .ok_or_else(|| deny("Actor binding is absent or outside the source Realm"))?;
        let deadline = permit.expires_at.min(Instant::now() + MAX_DURATION);
        let deadline = scope.map_or(deadline, |scope| deadline.min(scope.deadline()));
        let dispatch_id = Arc::new(OnceLock::new());
        let call = async {
            let timeout_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis() as i64;
            if timeout_ms == 0 {
                return Err(ActrError::TimedOut);
            }
            match bridge(HostOperation::ScopedActorCall {
                request: HostCallRawV1 {
                    target,
                    route_key: route,
                    payload,
                    timeout_ms,
                    payload_type: PayloadType::RpcReliable as i32,
                },
                deadline,
                dispatch_id: dispatch_id.clone(),
            })
            .await
            {
                HostOperationResult::Bytes(bytes) if bytes.len() <= MAX_RESPONSE => Ok(bytes),
                HostOperationResult::Error(error) => Err(error),
                _ => Err(deny("Actor response failed or exceeds its limit")),
            }
        };
        run_permitted(&permit, deadline, scope, Some(dispatch_id.as_ref()), call).await
    }

    pub(crate) async fn https_request(
        &self,
        invocation: &InvocationContext,
        alias: &str,
        body: Vec<u8>,
        scope: Option<&RequestScope>,
    ) -> ActorResult<Vec<u8>> {
        if scope.is_some_and(|scope| !scope.is_active()) {
            return Err(ActrError::TimedOut);
        }
        let _slot = self
            .concurrency
            .try_acquire()
            .map_err(|_| deny("egress concurrency limit"))?;
        let rule = self
            .permissions
            .https
            .get(alias)
            .ok_or_else(|| deny("HTTPS alias is outside the signed scope"))?;
        if body.len() > MAX_REQUEST
            || (!body.is_empty() && matches!(rule.method.as_str(), "GET" | "HEAD"))
        {
            return Err(deny("HTTPS request body violates its scope"));
        }
        let permit = self.policy.authorize(EgressRequest {
            invocation,
            source_package_sha256: &self.digest,
            privacy_contract_sha256: &self.contract_sha256,
            data_classes: &rule.data_classes,
            alias,
            operation: EgressOperation::Https {
                url: &rule.url,
                method: &rule.method,
                required: rule.required,
            },
        })?;
        let deadline = permit.expires_at.min(Instant::now() + MAX_DURATION);
        let deadline = scope.map_or(deadline, |scope| deadline.min(scope.deadline()));
        let dispatch_id = OnceLock::new();
        run_permitted(
            &permit,
            deadline,
            scope,
            Some(&dispatch_id),
            fixed_https(
                rule,
                body,
                &dispatch_id,
                || {
                    if scope.is_some_and(|scope| !scope.is_active()) {
                        return Err(ActrError::TimedOut);
                    }
                    if permit.revocation.is_cancelled() || Instant::now() >= deadline {
                        return Err(deny("HTTPS grant inactive before send"));
                    }
                    Ok(())
                },
                #[cfg(test)]
                self.https_fixture.as_ref(),
            ),
        )
        .await
    }
}

async fn run_permitted<T>(
    permit: &EgressPermit,
    deadline: Instant,
    scope: Option<&RequestScope>,
    dispatch_id: Option<&OnceLock<String>>,
    operation: impl std::future::Future<Output = ActorResult<T>>,
) -> ActorResult<T> {
    let inactive = |cause: &str| match dispatch_id.and_then(OnceLock::get) {
        Some(operation_id) => ActrError::DeliveryUncertain {
            operation_id: operation_id.clone(),
            cause: cause.to_owned(),
        },
        None if scope.is_some_and(|scope| !scope.is_active()) => ActrError::TimedOut,
        None => deny(cause),
    };
    if Instant::now() >= deadline {
        return Err(inactive("egress grant expired or timed out"));
    }
    tokio::select! {
        biased;
        _ = RequestScope::wait_cancelled(scope) => Err(inactive("request scope cancelled")),
        _ = permit.revocation.cancelled() => Err(inactive("egress grant revoked")),
        _ = crate::timer::sleep_until(crate::timer::ids::WASM_EGRESS_DEADLINE, deadline) => {
            Err(inactive("egress grant expired or timed out"))
        },
        result = operation => {
            if scope.is_some_and(|scope| !scope.is_active())
                || permit.revocation.is_cancelled()
                || Instant::now() >= deadline
            {
                Err(inactive("egress result belongs to an inactive grant"))
            } else {
                result
            }
        }
    }
}

#[cfg(test)]
struct HttpsFixture {
    address: std::net::SocketAddr,
    root: reqwest::Certificate,
}

async fn fixed_https(
    rule: &HttpsPermission,
    body: Vec<u8>,
    dispatch_id: &OnceLock<String>,
    check_before_send: impl FnOnce() -> ActorResult<()>,
    #[cfg(test)] fixture: Option<&HttpsFixture>,
) -> ActorResult<Vec<u8>> {
    let url = url::Url::parse(&rule.url).map_err(|_| deny("invalid signed HTTPS URL"))?;
    let Some(url::Host::Domain(domain)) = url.host() else {
        return Err(deny("HTTPS requires a DNS host"));
    };
    if url.scheme() != "https" {
        return Err(deny("request must use HTTPS"));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| deny("HTTPS has no port"))?;
    #[cfg(not(test))]
    let test_address = None::<std::net::SocketAddr>;
    #[cfg(test)]
    let mut test_address = None::<std::net::SocketAddr>;
    #[cfg(test)]
    if let Some(fixture) = fixture {
        if domain != "localhost" || port != fixture.address.port() {
            return Err(deny("HTTPS target is outside its configured fixture"));
        }
        test_address = Some(fixture.address);
    }
    if port != 443 && test_address.is_none() {
        return Err(deny("HTTPS requires port 443"));
    }
    let addresses: Vec<_> = match test_address {
        Some(address) => vec![address],
        None => tokio::net::lookup_host((domain, port))
            .await
            .map_err(|_| deny("HTTPS name lookup failed"))?
            .collect(),
    };
    if addresses.is_empty()
        || addresses.iter().any(|address| {
            Some(*address) != test_address && actr_protocol::uri::is_forbidden_address(address.ip())
        })
    {
        return Err(deny(
            "HTTPS target address is outside the public network policy",
        ));
    }
    let builder = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(domain, &addresses);
    #[cfg(test)]
    let builder = if let Some(fixture) = fixture {
        builder.add_root_certificate(fixture.root.clone())
    } else {
        builder
    };
    let client = builder
        .build()
        .map_err(|_| deny("HTTPS client construction failed"))?;
    let method = reqwest::Method::from_bytes(rule.method.as_bytes())
        .map_err(|_| deny("invalid signed method"))?;
    let request = client.request(method, url);
    let request = if body.is_empty() {
        request
    } else {
        request.body(body)
    };
    check_before_send()?;
    if !matches!(rule.method.as_str(), "GET" | "HEAD" | "OPTIONS") {
        let _ = dispatch_id.set(uuid::Uuid::new_v4().to_string());
    }
    let mut response = request.send().await.map_err(|error| {
        if error.is_connect() || error.is_builder() {
            deny("HTTPS request failed before sending")
        } else {
            https_failure(dispatch_id, "HTTPS request failed")
        }
    })?;
    if response.status().is_server_error() || response.status().is_redirection() {
        return Err(https_failure(
            dispatch_id,
            "HTTPS returned an ambiguous response",
        ));
    }
    if !response.status().is_success() {
        return Err(deny("HTTPS returned a non-success response"));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE as u64)
    {
        return Err(https_failure(
            dispatch_id,
            "HTTPS response exceeds its limit",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| https_failure(dispatch_id, "HTTPS response read failed"))?
    {
        if chunk.len() > MAX_RESPONSE - bytes.len() {
            return Err(https_failure(
                dispatch_id,
                "HTTPS response exceeds its limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn https_failure(dispatch_id: &OnceLock<String>, cause: &str) -> ActrError {
    match dispatch_id.get() {
        Some(operation_id) => ActrError::DeliveryUncertain {
            operation_id: operation_id.clone(),
            cause: cause.to_owned(),
        },
        None => deny(cause),
    }
}

fn deny(reason: &str) -> ActrError {
    ActrError::PermissionDenied(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestPolicy {
        target: ActrId,
        revocation: CancellationToken,
    }

    impl EgressPolicy for TestPolicy {
        fn authorize(&self, request: EgressRequest<'_>) -> ActorResult<EgressPermit> {
            match request.operation {
                EgressOperation::Actor {
                    target_package_sha256,
                    ..
                } if request.invocation.request_id == "approved"
                    && target_package_sha256 == "aa".repeat(32)
                    && request.privacy_contract_sha256 == "cc".repeat(32)
                    && request.data_classes == ["private"] => {}
                _ => return Err(deny("test policy denied")),
            }
            Ok(EgressPermit {
                expires_at: Instant::now() + Duration::from_secs(5),
                revocation: self.revocation.clone(),
                target: Some(self.target.clone()),
            })
        }

        fn authorize_inbound(
            &self,
            _: &InvocationContext,
            _: &str,
            _: &str,
            _: &str,
        ) -> ActorResult<()> {
            Err(deny("not used by this fixture"))
        }
    }

    #[tokio::test]
    async fn scoped_route_and_revocation_guard_the_host_bridge() {
        let route = "notes.Index.GetTask";
        let contract = PackagePrivacyContract {
            sha256: "cc".repeat(32),
            egress: EgressPermissions {
                actor: BTreeMap::from([(
                    "index".into(),
                    actr_pack::egress::ActorPermission {
                        rpc_routes: vec![route.into()],
                        data_classes: vec!["private".into()],
                        target_actor_type: "notes.example/Index@1.0.0".into(),
                    },
                )]),
                ..Default::default()
            },
            dependency_digests: BTreeMap::new(),
        };
        let lock: actr_config::lock::LockFile = format!(
            r#"
[metadata]
version = 2
generated_at = "2026-11-19T00:00:00Z"
[[dependency]]
name = "index"
alias = "index"
actr_type = "notes.example/Index@1.0.0"
fingerprint = "service_semantic:fixture"
package-sha256 = "{}"
cached_at = "2026-11-19T00:00:00Z"
files = []
"#,
            "aa".repeat(32)
        )
        .parse()
        .unwrap();
        let target = ActrId {
            realm: "test@notes.example".into(),
            serial_number: 2,
        };
        let mut invocation = InvocationContext {
            self_id: ActrId {
                serial_number: 1,
                ..target.clone()
            },
            caller_id: None,
            request_id: "denied".into(),
        };
        let revocation = CancellationToken::new();
        let policy = Arc::new(TestPolicy {
            target: target.clone(),
            revocation: revocation.clone(),
        });
        let broker = EgressBroker::new(policy, "bb".repeat(32), contract, Some(&lock));
        let count = Arc::new(AtomicUsize::new(0));
        let scope = RequestScope::root_at(Instant::now() + Duration::from_secs(2));
        let scope_deadline = scope.deadline();
        let bridge: HostAbiFn = {
            let count = count.clone();
            Arc::new(move |operation| {
                let count = count.clone();
                let target = target.clone();
                Box::pin(async move {
                    let HostOperation::ScopedActorCall {
                        request, deadline, ..
                    } = operation
                    else {
                        panic!("unexpected bridge operation");
                    };
                    assert_eq!(request.target, target);
                    assert_eq!(request.route_key, route);
                    assert_eq!(deadline, scope_deadline);
                    count.fetch_add(1, Ordering::Relaxed);
                    HostOperationResult::Bytes(vec![42])
                })
            })
        };
        assert!(
            broker
                .actor_call(
                    &invocation,
                    bridge.clone(),
                    "index",
                    route.into(),
                    vec![],
                    Some(&scope),
                )
                .await
                .is_err()
        );
        invocation.request_id = "approved".into();
        assert!(
            broker
                .actor_call(
                    &invocation,
                    bridge.clone(),
                    "index",
                    "notes.Index.Forbidden".into(),
                    vec![],
                    Some(&scope),
                )
                .await
                .is_err()
        );
        assert_eq!(
            broker
                .actor_call(
                    &invocation,
                    bridge.clone(),
                    "index",
                    route.into(),
                    vec![],
                    Some(&scope),
                )
                .await
                .unwrap(),
            vec![42]
        );
        let dispatched_bridge: HostAbiFn = Arc::new(move |operation| {
            let revocation = revocation.clone();
            Box::pin(async move {
                let HostOperation::ScopedActorCall { dispatch_id, .. } = operation else {
                    panic!("unexpected bridge operation");
                };
                dispatch_id.set("sent-request".into()).unwrap();
                revocation.cancel();
                std::future::pending().await
            })
        });
        let error = broker
            .actor_call(
                &invocation,
                dispatched_bridge,
                "index",
                route.into(),
                vec![],
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ActrError::DeliveryUncertain { operation_id, .. } if operation_id == "sent-request"
        ));
        assert!(
            broker
                .actor_call(&invocation, bridge, "index", route.into(), vec![], None)
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}

#[cfg(test)]
mod notebook_test;
