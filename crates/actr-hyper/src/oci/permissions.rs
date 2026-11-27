//! Apply signed package permissions and deployment grants to OCI Host operations.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use actr_config::{OciActorGrant, OciRuntimeConfig, lock::LockFile};
use actr_framework::Dest;
use actr_pack::egress::{ActorPermission, PackagePrivacyContract};
use actr_protocol::oci::{HostOperation, HostRequest};
use actr_protocol::{ActorResult, ActrError, ActrId, ActrType, PayloadType};
use bytes::Bytes;
use prost::Message;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::context::RuntimeContext;
use crate::egress::{
    EgressBroker, EgressOperation, EgressPermit, EgressPolicy, EgressRequest, run_permitted,
};
use crate::request_scope::RequestScope;
use crate::transport::RpcDeadline;
use crate::workload::InvocationContext;

const BODY_LIMIT: usize = 64 * 1024;
const RESPONSE_LIMIT: usize = 256 * 1024;
const EGRESS_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct Permissions {
    deployment: Arc<OciRuntimeConfig>,
    policy: Arc<dyn EgressPolicy>,
    https: EgressBroker,
    package_digest: String,
    contract_digest: String,
    actor: BTreeMap<String, ScopedDependency>,
}

struct ScopedDependency {
    permission: ActorPermission,
    lock: actr_config::lock::LockedDependency,
}

impl Permissions {
    pub fn new(
        deployment: Arc<OciRuntimeConfig>,
        custom: Option<Arc<dyn EgressPolicy>>,
        digest: String,
        contract: PackagePrivacyContract,
        lock: Option<&LockFile>,
    ) -> Self {
        let actor = lock
            .into_iter()
            .flat_map(|lock| &lock.dependencies)
            .filter_map(|entry| {
                let permission = contract.egress.actor.get(&entry.alias)?;
                Some((
                    entry.alias.clone(),
                    ScopedDependency {
                        permission: ActorPermission {
                            rpc_routes: permission.rpc_routes.clone(),
                            data_classes: permission.data_classes.clone(),
                            target_actor_type: permission.target_actor_type.clone(),
                        },
                        lock: entry.clone(),
                    },
                ))
            })
            .collect();
        let policy = custom.unwrap_or_else(|| Arc::new(FilePolicy(Arc::clone(&deployment))));
        let contract_digest = contract.sha256.clone();
        let https = EgressBroker::new(Arc::clone(&policy), digest.clone(), contract, lock);
        Self {
            deployment,
            policy,
            https,
            package_digest: digest,
            contract_digest,
            actor,
        }
    }

    pub async fn handle(
        &self,
        request: HostRequest,
        context: RuntimeContext,
        invocation: InvocationContext,
        scope: Arc<RequestScope>,
    ) -> ActorResult<Vec<u8>> {
        if !scope.is_active() {
            return Err(ActrError::TimedOut);
        }
        if request.payload.len() > BODY_LIMIT {
            return Err(denied("OCI request exceeds the egress payload limit"));
        }
        let operation = request.operation();
        if operation == HostOperation::Https {
            return self
                .https
                .https_request(
                    &invocation,
                    &request.alias,
                    request.payload,
                    Some(&scope),
                    Arc::new(OnceLock::new()),
                )
                .await;
        }

        let (alias, dependency, permit) = self.bind(&request, &invocation, operation)?;
        let target = permit
            .target
            .clone()
            .ok_or_else(|| denied("OCI Actor target is not bound"))?;
        if target == invocation.self_id || target.realm != invocation.self_id.realm {
            return Err(denied(
                "OCI Actor target is outside the permitted Realm or is the caller",
            ));
        }
        let operation_deadline = if request.timeout_ms > 0 {
            Instant::now() + EGRESS_TIMEOUT.min(Duration::from_millis(request.timeout_ms as u64))
        } else {
            Instant::now() + EGRESS_TIMEOUT
        };
        let deadline = permit
            .expires_at
            .min(operation_deadline)
            .min(scope.deadline());
        let dispatch_id = OnceLock::new();
        let route = request.route;
        let payload = Bytes::from(request.payload);

        let call = async {
            match operation {
                HostOperation::Call => {
                    let bytes = context
                        .call_raw_with_deadline(
                            &Dest::Peer(target),
                            route,
                            PayloadType::RpcReliable,
                            payload,
                            RpcDeadline::at(deadline),
                            Some(&dispatch_id),
                        )
                        .await?;
                    if bytes.len() > RESPONSE_LIMIT {
                        return Err(ActrError::DeliveryUncertain {
                            operation_id: dispatch_id
                                .get()
                                .cloned()
                                .unwrap_or_else(|| invocation.request_id.clone()),
                            cause: "OCI Actor response exceeds its limit".into(),
                        });
                    }
                    Ok(bytes.to_vec())
                }
                HostOperation::Tell => {
                    if !self
                        .deployment
                        .actors
                        .iter()
                        .any(|grant| grant.alias == alias && grant.allow_tell)
                    {
                        return Err(denied("OCI Actor Tell is not granted"));
                    }
                    let _ = dispatch_id.set(uuid::Uuid::new_v4().to_string());
                    context
                        .tell_raw(
                            &Dest::Peer(target),
                            route,
                            PayloadType::RpcReliable,
                            payload,
                        )
                        .await?;
                    Ok(Vec::new())
                }
                HostOperation::Resolve => {
                    Ok(ActrType::from_string_repr(&dependency.lock.actr_type)
                        .map_err(|error| ActrError::Internal(error.to_string()))?
                        .encode_to_vec())
                }
                HostOperation::Discover => {
                    let actor_type = ActrType::from_string_repr(&dependency.lock.actr_type)
                        .map_err(|error| ActrError::Internal(error.to_string()))?;
                    let discovered = context.discover_service(&actor_type).await?;
                    if discovered != target {
                        return Err(denied("discovery did not resolve the authorized OCI Actor"));
                    }
                    Ok(target.encode_to_vec())
                }
                _ => Err(denied("unknown OCI host operation")),
            }
        };
        run_permitted(&permit, deadline, Some(&scope), Some(&dispatch_id), call).await
    }

    fn bind<'a>(
        &'a self,
        request: &HostRequest,
        invocation: &InvocationContext,
        operation: HostOperation,
    ) -> ActorResult<(&'a str, &'a ScopedDependency, EgressPermit)> {
        let mut matches =
            self.actor.iter().filter_map(|(alias, dependency)| {
                if !request.alias.is_empty() && request.alias != *alias {
                    return None;
                }
                if request.actor_type.as_ref().is_some_and(|actor_type| {
                    actor_type.to_string_repr() != dependency.lock.actr_type
                }) {
                    return None;
                }
                let package_sha256 = dependency.lock.package_sha256.as_deref()?;
                let permit = dependency.permission.rpc_routes.iter().find_map(|route| {
                    let allowed = match operation {
                        HostOperation::Call | HostOperation::Tell => route == &request.route,
                        HostOperation::Resolve => !request.alias.is_empty(),
                        HostOperation::Discover => request.actor_type.is_some(),
                        _ => false,
                    };
                    if !allowed {
                        return None;
                    }
                    let permit = self
                        .policy
                        .authorize(EgressRequest {
                            invocation,
                            source_package_sha256: &self.package_digest,
                            privacy_contract_sha256: &self.contract_digest,
                            data_classes: &dependency.permission.data_classes,
                            alias,
                            operation: EgressOperation::Actor {
                                route,
                                target_package_sha256: package_sha256,
                                target_actor_type: &dependency.lock.actr_type,
                                contract_fingerprint: &dependency.lock.fingerprint,
                            },
                        })
                        .ok()?;
                    let target = permit.target.as_ref()?;
                    if request
                        .target
                        .as_ref()
                        .is_some_and(|requested| requested != target)
                    {
                        return None;
                    }
                    Some(permit)
                })?;
                Some((alias.as_str(), dependency, permit))
            });
        let first = matches
            .next()
            .ok_or_else(|| denied("OCI dependency or operation is not authorized"))?;
        if matches.next().is_some() {
            return Err(denied(
                "OCI request matches more than one authorized dependency",
            ));
        }
        Ok(first)
    }
}

struct FilePolicy(Arc<OciRuntimeConfig>);

impl EgressPolicy for FilePolicy {
    fn authorize(&self, request: EgressRequest<'_>) -> ActorResult<EgressPermit> {
        if self.0.package_sha256 != request.source_package_sha256 {
            return Err(denied("OCI egress grant does not match the source package"));
        }
        let target = match request.operation {
            EgressOperation::Https { url, method, .. } => {
                if !self.0.https.iter().any(|grant| {
                    grant.alias == request.alias
                        && grant.url == url
                        && grant.method == method
                        && request
                            .data_classes
                            .iter()
                            .all(|class| grant.data_classes.contains(class))
                }) {
                    return Err(denied("HTTPS terminal is outside the OCI deployment grant"));
                }
                None
            }
            EgressOperation::Actor {
                route,
                target_package_sha256,
                target_actor_type,
                contract_fingerprint,
            } => {
                let grant = self
                    .0
                    .actors
                    .iter()
                    .find(|grant| {
                        grant.alias == request.alias
                            && grant.package_sha256 == target_package_sha256
                            && grant.contract_fingerprint == contract_fingerprint
                            && grant.routes.iter().any(|allowed| allowed == route)
                            && request
                                .data_classes
                                .iter()
                                .all(|class| grant.data_classes.contains(class))
                    })
                    .ok_or_else(|| denied("Actor route is outside the OCI deployment grant"))?;
                Some(granted_target(grant, target_actor_type)?)
            }
        };
        Ok(EgressPermit {
            expires_at: Instant::now() + EGRESS_TIMEOUT,
            revocation: CancellationToken::new(),
            target,
        })
    }

    fn authorize_inbound(
        &self,
        _: &InvocationContext,
        _: &str,
        _: &str,
        _: &str,
    ) -> ActorResult<()> {
        Ok(())
    }
}

fn granted_target(grant: &OciActorGrant, expected_type: &str) -> ActorResult<ActrId> {
    let actor = ActrId::from_string_repr(&grant.target)
        .map_err(|_| denied("OCI deployment has an invalid Actor ID"))?;
    if grant.actor_type != expected_type {
        return Err(denied(
            "OCI Actor target does not match the signed dependency type",
        ));
    }
    Ok(actor)
}

fn denied(message: &str) -> ActrError {
    ActrError::PermissionDenied(message.into())
}
