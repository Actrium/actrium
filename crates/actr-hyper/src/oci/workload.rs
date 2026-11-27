//! Translate Hyper lifecycle and invocation scopes into OCI Guest sessions.

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

use actr_config::{OciRuntimeConfig, lock::LockFile};
use actr_framework::{
    Context as _, PeerEvent, WebRtcNetworkPath, WebRtcPeerStatus, WebRtcUnavailableReason,
};
use actr_pack::oci::{ACTOR_RECEIVE, OciWorkloadDescriptor, VerifiedOciPackage};
use actr_protocol::oci as wire;
use actr_protocol::{ActorResult, ActrError, RpcEnvelope};
use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use futures_util::{StreamExt, stream::FuturesUnordered};
use prost::Message;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::{Semaphore, SemaphorePermit, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::context::RuntimeContext;
use crate::egress::EgressPolicy;
use crate::request_scope::{CancelReason, RequestScope};
use crate::workload::{InvocationContext, PackageHookEvent};

use super::image::{self, ImageLease};
use super::permissions::Permissions;
use super::runtime::{self, Container, GuestFailure};

const START_TIMEOUT: Duration = Duration::from_secs(15);
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(30);
const HOOK_TIMEOUT: Duration = Duration::from_secs(5);
static INSTANCE_BUDGET: Semaphore = Semaphore::const_new(16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Starting,
    Ready,
    Stopping,
    Failed,
}

struct InboundPolicy {
    policy: Arc<dyn EgressPolicy>,
    package_digest: String,
    privacy_digest: String,
}

pub(crate) struct Admission {
    routes: BTreeSet<String>,
    inbound: bool,
    policy: Option<InboundPolicy>,
    status: watch::Sender<Status>,
    failure: OnceLock<GuestFailure>,
    memory_limit: i64,
}

impl Admission {
    pub(crate) fn status(&self) -> watch::Receiver<Status> {
        self.status.subscribe()
    }

    pub(crate) fn check(&self, route: &str) -> ActorResult<()> {
        if *self.status.borrow() != Status::Ready {
            return Err(self
                .failure_error()
                .unwrap_or_else(|| unavailable("OCI workload is not ready")));
        }
        if !self.inbound {
            return Err(ActrError::PermissionDenied(
                "OCI package has no inbound RPC grant".into(),
            ));
        }
        if !self.routes.contains(route) {
            return Err(ActrError::UnknownRoute(route.to_owned()));
        }
        Ok(())
    }

    pub(crate) fn authorize_inbound(
        &self,
        invocation: &InvocationContext,
        route: &str,
    ) -> ActorResult<()> {
        if let Some(binding) = &self.policy {
            binding.policy.authorize_inbound(
                invocation,
                &binding.package_digest,
                &binding.privacy_digest,
                route,
            )?;
        }
        Ok(())
    }

    pub(crate) fn failure_error(&self) -> Option<ActrError> {
        self.failure
            .get()
            .map(|failure| unavailable(failure.describe(self.memory_limit)))
    }

    fn failed(&self, failure: GuestFailure) {
        self.status.send_if_modified(|status| {
            if matches!(*status, Status::Stopping | Status::Failed) {
                return false;
            }
            let _ = self.failure.set(failure);
            *status = Status::Failed;
            true
        });
    }
}

pub(crate) struct OciWorkload {
    config: Arc<OciRuntimeConfig>,
    descriptor: OciWorkloadDescriptor,
    capabilities: Vec<String>,
    permissions: Option<Arc<Permissions>>,
    image: Option<ImageLease>,
    session: Option<Session>,
    cleanup: Option<JoinHandle<ActorResult<()>>>,
    startup_deadline: Option<Instant>,
    pub(crate) admission: Arc<Admission>,
    _instance_budget: SemaphorePermit<'static>,
}

impl std::fmt::Debug for OciWorkload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OciWorkload")
            .field("digest", &self.descriptor.digest)
            .finish_non_exhaustive()
    }
}

impl OciWorkload {
    pub(crate) async fn prepare(
        config: Arc<OciRuntimeConfig>,
        package: &VerifiedOciPackage,
        lock: Option<&LockFile>,
        routes: BTreeSet<String>,
        policy: Option<Arc<dyn EgressPolicy>>,
    ) -> Result<Self> {
        let package_digest = hex::encode(package.archive_digest());
        ensure!(
            package_digest == config.package_sha256,
            "OCI archive does not match the host grant"
        );
        let manifest = package.manifest();
        let capabilities = manifest.host.required_capabilities.clone();
        ensure!(
            capabilities
                .iter()
                .all(|capability| config.allowed_capabilities.contains(capability)),
            "OCI deployment does not grant all required capabilities"
        );
        let privacy = package.privacy_contract()?;
        if policy.is_none() {
            if let Some(contract) = &privacy {
                for (alias, terminal) in &contract.egress.https {
                    if terminal.required {
                        ensure!(
                            config.https.iter().any(|grant| {
                                grant.alias == *alias
                                    && grant.url == terminal.url
                                    && grant.method == terminal.method
                                    && terminal
                                        .data_classes
                                        .iter()
                                        .all(|class| grant.data_classes.contains(class))
                            }),
                            "required HTTPS terminal {alias} has no OCI deployment grant"
                        );
                    }
                }
                for alias in contract.egress.actor.keys() {
                    ensure!(
                        config.actors.iter().any(|grant| grant.alias == *alias),
                        "required Actor dependency {alias} has no OCI deployment grant"
                    );
                }
            }
        }
        let inbound_policy = policy.as_ref().map(|policy| InboundPolicy {
            policy: Arc::clone(policy),
            package_digest: package_digest.clone(),
            // The hook also runs for packages without an egress contract.
            privacy_digest: privacy
                .as_ref()
                .map(|contract| contract.sha256.clone())
                .unwrap_or_default(),
        });
        let permissions = privacy.map(|contract| {
            Arc::new(Permissions::new(
                Arc::clone(&config),
                policy,
                package_digest,
                contract,
                lock,
            ))
        });
        let instance_budget = INSTANCE_BUDGET
            .try_acquire()
            .context("OCI host instance budget exhausted")?;
        runtime::doctor(&config).await?;
        runtime::data_grant(&config, &capabilities)?;
        super::gc::reconcile(&config).await?;

        let image = image::prepare(&config, package).await?;
        let descriptor = manifest
            .workload
            .oci
            .clone()
            .context("missing signed OCI workload")?;
        let admission = Arc::new(Admission {
            routes,
            inbound: capabilities
                .iter()
                .any(|capability| capability == ACTOR_RECEIVE),
            policy: inbound_policy,
            status: watch::channel(Status::Starting).0,
            failure: OnceLock::new(),
            memory_limit: config.memory_limit_bytes,
        });
        Ok(Self {
            config,
            descriptor,
            capabilities,
            permissions,
            image: Some(image),
            session: None,
            cleanup: None,
            startup_deadline: None,
            admission,
            _instance_budget: instance_budget,
        })
    }

    pub(crate) async fn on_start(
        &mut self,
        ctx: RuntimeContext,
        invocation: InvocationContext,
    ) -> ActorResult<()> {
        let deadline = Instant::now() + START_TIMEOUT;
        self.startup_deadline = Some(deadline);
        let image = self
            .image
            .take()
            .ok_or_else(|| unavailable("OCI image lease was consumed"))?;
        let result =
            crate::timer::timeout_at(crate::timer::ids::OCI_WORKLOAD_START, deadline, async {
                let container =
                    Container::start(&self.config, &self.descriptor, &self.capabilities, image)
                        .await?;
                Session::connect(
                    container,
                    invocation.self_id.clone(),
                    self.capabilities.clone(),
                    Arc::clone(&self.admission),
                )
                .await
            })
            .await;
        self.session = match result {
            Ok(Ok(session)) => Some(session),
            Ok(Err(error)) => {
                self.admission.failed(GuestFailure::Disconnected);
                return Err(unavailable(error.to_string()));
            }
            Err(_) => {
                self.admission.failed(GuestFailure::Disconnected);
                return Err(ActrError::TimedOut);
            }
        };
        self.invoke(
            wire::InvocationKind::Start,
            invocation,
            Some(ctx),
            None,
            None,
            deadline,
        )
        .await
        .map(drop)
    }

    pub(crate) async fn on_ready(
        &mut self,
        ctx: RuntimeContext,
        invocation: InvocationContext,
    ) -> ActorResult<()> {
        let deadline = self
            .startup_deadline
            .ok_or_else(|| unavailable("OCI workload was not started"))?;
        self.invoke(
            wire::InvocationKind::Ready,
            invocation,
            Some(ctx),
            None,
            None,
            deadline,
        )
        .await?;
        self.admission.status.send_if_modified(|status| {
            if *status == Status::Starting {
                *status = Status::Ready;
                true
            } else {
                false
            }
        });
        if *self.admission.status.borrow() != Status::Ready {
            return Err(self
                .admission
                .failure_error()
                .unwrap_or_else(|| unavailable("OCI Guest disconnected before readiness")));
        }
        Ok(())
    }

    pub(crate) async fn on_stop(
        &mut self,
        ctx: RuntimeContext,
        invocation: InvocationContext,
    ) -> ActorResult<()> {
        let previous = self.admission.status.send_replace(Status::Stopping);
        if previous != Status::Ready {
            return Ok(());
        }
        let deadline = Instant::now() + HOOK_TIMEOUT;
        self.invoke(
            wire::InvocationKind::Stop,
            invocation,
            Some(ctx),
            None,
            None,
            deadline,
        )
        .await?;
        if let Some(session) = &mut self.session {
            let sequence = session.next();
            let body = async {
                session
                    .send(sequence, wire::Body::Shutdown(wire::Empty {}))
                    .await?;
                let frame = session.receive().await?;
                if frame.sequence == sequence
                    && matches!(frame.body, Some(wire::Body::ShutdownAck(_)))
                {
                    Ok(())
                } else {
                    Err(unavailable("invalid OCI shutdown acknowledgment"))
                }
            };
            crate::timer::timeout_at(crate::timer::ids::OCI_WORKLOAD_SHUTDOWN_ACK, deadline, body)
                .await
                .map_err(|_| ActrError::TimedOut)??;
        }
        Ok(())
    }

    pub(crate) async fn dispatch(
        &mut self,
        envelope: RpcEnvelope,
        ctx: RuntimeContext,
        invocation: InvocationContext,
    ) -> ActorResult<Bytes> {
        self.admission.check(&envelope.route_key)?;
        self.admission
            .authorize_inbound(&invocation, &envelope.route_key)?;
        let deadline = Instant::now() + DISPATCH_TIMEOUT;
        self.invoke(
            wire::InvocationKind::Dispatch,
            invocation,
            Some(ctx),
            Some(envelope),
            None,
            deadline,
        )
        .await
        .map(Bytes::from)
    }

    pub(crate) async fn observe(
        &mut self,
        event: PackageHookEvent,
        context: Option<RuntimeContext>,
        invocation: InvocationContext,
    ) -> ActorResult<()> {
        self.invoke(
            wire::InvocationKind::Observe,
            invocation,
            context,
            None,
            Some(observation(event)),
            Instant::now() + HOOK_TIMEOUT,
        )
        .await
        .map(drop)
    }

    pub(crate) async fn shutdown(&mut self) -> ActorResult<()> {
        self.admission.status.send_replace(Status::Stopping);
        if let Some(cleanup) = self.cleanup.take() {
            cleanup
                .await
                .map_err(|error| unavailable(error.to_string()))??;
        }
        if let Some(mut session) = self.session.take() {
            session.reader.abort();
            session
                .container
                .stop()
                .await
                .map_err(|error| unavailable(error.to_string()))?;
        }
        Ok(())
    }

    async fn invoke(
        &mut self,
        kind: wire::InvocationKind,
        invocation: InvocationContext,
        mut context: Option<RuntimeContext>,
        envelope: Option<RpcEnvelope>,
        observation: Option<wire::Observation>,
        deadline: Instant,
    ) -> ActorResult<Vec<u8>> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| unavailable("OCI Guest is unavailable"))?;
        let scope = match context.as_ref().and_then(|ctx| ctx.scope.as_ref()) {
            Some(parent) => parent.child_at(deadline),
            None => RequestScope::root_at(deadline),
        };
        if !scope.is_active() {
            return Err(ActrError::TimedOut);
        }
        if let Some(ctx) = &mut context {
            ctx.scope = Some(Arc::clone(&scope));
        }
        let token = uuid::Uuid::new_v4().as_bytes().to_vec();
        let wire_invocation = wire::Invocation {
            token: token.clone(),
            caller: invocation.caller_id.clone(),
            request_id: invocation.request_id.clone(),
            expects_response: context.as_ref().is_none_or(|ctx| ctx.expects_response()),
            kind: kind as i32,
            envelope,
            observation,
        };
        let sequence = session.next();
        let frame = session.frame(sequence, wire::Body::Invoke(wire_invocation));
        if frame.encoded_len() > wire::MAX_FRAME_BYTES {
            return Err(ActrError::InvalidArgument(
                "OCI invocation exceeds the IPC frame limit".into(),
            ));
        }
        let mut invoke_started = false;
        let body = async {
            invoke_started = true;
            session.send_frame(&frame).await?;
            let mut calls = FuturesUnordered::new();
            loop {
                tokio::select! {
                    biased;
                    frame = session.receive() => {
                        let frame = frame?;
                        match frame.body {
                            Some(wire::Body::InvokeResult(result)) if frame.sequence == sequence => return Ok(result),
                            Some(wire::Body::HostRequest(request)) => {
                                if frame.sequence <= session.last_guest_request {
                                    return Err(unavailable("invalid OCI host request sequence"));
                                }
                                session.last_guest_request = frame.sequence;
                                if request.token != token {
                                    let result = Err::<Vec<u8>, _>(ActrError::PermissionDenied("OCI invocation has ended".into()));
                                    session.send(frame.sequence, wire::Body::HostResult(result.into())).await?;
                                    continue;
                                }
                                if calls.len() >= wire::MAX_HOST_REQUESTS {
                                    return Err(unavailable("OCI Guest exceeded its host operation limit"));
                                }
                                if request.payload.len() > 64 * 1024 {
                                    let result = Err::<Vec<u8>, _>(ActrError::PermissionDenied("OCI host request body exceeds its limit".into()));
                                    session.send(frame.sequence, wire::Body::HostResult(result.into())).await?;
                                    continue;
                                }
                                let ctx = context.clone();
                                let scope = Arc::clone(&scope);
                                let invocation = invocation.clone();
                                let permissions = self.permissions.as_deref();
                                calls.push(async move {
                                    let outcome = match (permissions, ctx) {
                                        (Some(permissions), Some(ctx)) => permissions.handle(request, ctx, invocation, scope).await,
                                        _ => Err(ActrError::PermissionDenied("host operation is outside this OCI invocation".into())),
                                    };
                                    (frame.sequence, outcome)
                                });
                            }
                            _ => return Err(unavailable("unexpected OCI IPC frame")),
                        }
                    }
                    Some((id, outcome)) = calls.next(), if !calls.is_empty() => {
                        session.send(id, wire::Body::HostResult(outcome.into())).await?;
                    }
                }
            }
        };
        let settled = tokio::select! {
            _ = crate::timer::sleep_until(crate::timer::ids::OCI_WORKLOAD_INVOKE, scope.deadline()) => Err(ActrError::TimedOut),
            _ = RequestScope::wait_cancelled(Some(&scope)) => Err(ActrError::TimedOut),
            output = body => output,
        };
        scope.cancel(CancelReason::CallerDropped);
        match settled {
            Ok(result) => result.into_actor_result(),
            // A cancelled scope before any IPC write leaves the current
            // session usable.
            Err(error) if !invoke_started => Err(error),
            Err(mut error) => {
                if let Some(mut session) = self.session.take() {
                    let failure = if matches!(error, ActrError::TimedOut) {
                        // Keep the caller's deadline. It may already have expired.
                        GuestFailure::Disconnected
                    } else {
                        match self.admission.failure.get() {
                            Some(failure) => *failure,
                            None => runtime::guest_failure(&mut session.container.exit()).await,
                        }
                    };
                    error = interrupted_invocation_error(
                        kind,
                        &invocation.request_id,
                        error,
                        failure,
                        self.config.memory_limit_bytes,
                    );
                    self.admission.failed(failure);
                    session.reader.abort();
                    self.cleanup = Some(tokio::spawn(async move {
                        session.container.stop().await.map_err(|error| {
                            tracing::warn!(%error, "failed to stop OCI workload");
                            unavailable(error.to_string())
                        })
                    }));
                }
                Err(error)
            }
        }
    }
}

fn interrupted_invocation_error(
    kind: wire::InvocationKind,
    request_id: &str,
    error: ActrError,
    failure: GuestFailure,
    memory_limit: i64,
) -> ActrError {
    let exit_cause = match failure {
        GuestFailure::Exited(_) => Some(failure.describe(memory_limit)),
        GuestFailure::Disconnected => None,
    };
    if kind == wire::InvocationKind::Dispatch {
        return ActrError::DeliveryUncertain {
            operation_id: request_id.to_owned(),
            cause: exit_cause.unwrap_or_else(|| error.to_string()),
        };
    }
    exit_cause.map_or(error, unavailable)
}

struct Session {
    container: Container,
    token: Vec<u8>,
    writer: OwnedWriteHalf,
    reader: JoinHandle<()>,
    incoming: mpsc::Receiver<wire::Frame>,
    sequence: u64,
    last_guest_request: u64,
}

impl Session {
    async fn connect(
        mut container: Container,
        actor: actr_protocol::ActrId,
        capabilities: Vec<String>,
        admission: Arc<Admission>,
    ) -> Result<Self> {
        let handshake = async {
            let mut stream = container
                .stream
                .take()
                .context("missing OCI Guest connection")?;
            let token = uuid::Uuid::new_v4().as_bytes().to_vec();
            let hello = wire::Hello {
                major: wire::MAJOR,
                minor: wire::MINOR,
                actor: Some(actor),
                capabilities: capabilities.clone(),
            };
            let frame = wire::Frame {
                session: token.clone(),
                sequence: 0,
                body: Some(wire::Body::Hello(hello)),
            };
            let mut exit = container.exit();
            let handshake = async {
                wire::write_frame(&mut stream, &frame).await?;
                wire::read_frame(&mut stream).await
            };
            let ack = tokio::select! {
                result = handshake => match result {
                    Ok(ack) => ack,
                    Err(error) => match runtime::guest_failure(&mut exit).await {
                        failure @ GuestFailure::Exited(_) => anyhow::bail!("{}", failure.describe(admission.memory_limit)),
                        GuestFailure::Disconnected => return Err(error.into()),
                    },
                },
                outcome = runtime::wait_for_guest_exit(&mut exit) => {
                    let failure = outcome.map_or(GuestFailure::Disconnected, GuestFailure::Exited);
                    anyhow::bail!("{}", failure.describe(admission.memory_limit));
                }
            };
            ensure!(
                ack.sequence == 0 && ack.session == token,
                "OCI Guest handshake identity mismatch"
            );
            let Some(wire::Body::HelloAck(ack)) = ack.body else {
                anyhow::bail!("expected OCI Guest handshake acknowledgment");
            };
            ensure!(
                ack.major == wire::MAJOR
                    && ack.minor == wire::MINOR
                    && ack.capabilities == capabilities,
                "OCI Guest protocol or capability set does not match"
            );
            Ok::<_, anyhow::Error>((stream, token, exit))
        }
        .await;
        let (stream, token, mut exit) = match handshake {
            Ok(connection) => connection,
            Err(error) => return Err(container.abort_start(error).await),
        };
        let (mut read, writer) = stream.into_split();
        let (send, incoming) = mpsc::channel(8);
        let session_token = token.clone();
        let reader = tokio::spawn(async move {
            let failure = loop {
                tokio::select! {
                    biased;
                    frame = wire::read_frame(&mut read) => {
                        match frame {
                            Ok(frame) => {
                                if frame.session != session_token || send.try_send(frame).is_err() {
                                    break GuestFailure::Disconnected;
                                }
                            }
                            Err(_) => break runtime::guest_failure(&mut exit).await,
                        }
                    }
                    outcome = runtime::wait_for_guest_exit(&mut exit) => {
                        break outcome.map_or(GuestFailure::Disconnected, GuestFailure::Exited);
                    }
                }
            };
            admission.failed(failure);
        });
        Ok(Self {
            container,
            token,
            writer,
            reader,
            incoming,
            sequence: 0,
            last_guest_request: 0,
        })
    }

    fn next(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }

    fn frame(&self, sequence: u64, body: wire::Body) -> wire::Frame {
        wire::Frame {
            session: self.token.clone(),
            sequence,
            body: Some(body),
        }
    }

    async fn send(&mut self, sequence: u64, body: wire::Body) -> ActorResult<()> {
        let frame = self.frame(sequence, body);
        self.send_frame(&frame).await
    }

    async fn send_frame(&mut self, frame: &wire::Frame) -> ActorResult<()> {
        wire::write_frame(&mut self.writer, frame)
            .await
            .map_err(|error| unavailable(error.to_string()))
    }

    async fn receive(&mut self) -> ActorResult<wire::Frame> {
        self.incoming
            .recv()
            .await
            .ok_or_else(|| unavailable("OCI Guest disconnected"))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

fn unavailable(message: impl Into<String>) -> ActrError {
    ActrError::Unavailable(message.into())
}

fn observation(event: PackageHookEvent) -> wire::Observation {
    use PackageHookEvent as Event;
    use wire::EventKind as Kind;
    let (kind, mut observation) = match event {
        Event::SignalingConnecting => (Kind::SignalingConnecting, wire::Observation::default()),
        Event::SignalingConnected => (Kind::SignalingConnected, wire::Observation::default()),
        Event::SignalingDisconnected => (Kind::SignalingDisconnected, wire::Observation::default()),
        Event::WebSocketConnecting(peer) => (Kind::WebsocketConnecting, observe_peer(peer)),
        Event::WebSocketConnected(peer) => (Kind::WebsocketConnected, observe_peer(peer)),
        Event::WebSocketDisconnected(peer) => (Kind::WebsocketDisconnected, observe_peer(peer)),
        Event::WebRtcConnecting(peer) => (Kind::WebrtcConnecting, observe_peer(peer)),
        Event::WebRtcConnected(peer) => (Kind::WebrtcConnected, observe_peer(peer)),
        Event::WebRtcDisconnected(peer) => (Kind::WebrtcDisconnected, observe_peer(peer)),
        Event::WebRtcStateChanged(state) => (
            Kind::WebrtcStateChanged,
            wire::Observation {
                peer: Some(state.peer),
                relayed: state.relayed,
                peer_status: Some(status_number(state.status)),
                network_path: match state.network_path {
                    WebRtcNetworkPath::Unknown => 0,
                    WebRtcNetworkPath::Direct => 1,
                    WebRtcNetworkPath::Relayed => 2,
                },
                pc_generation: state.pc_generation,
                revision: state.revision,
                sendable: state.sendable,
                unavailable_reason: state.unavailable_reason.and_then(reason_number),
                ..Default::default()
            },
        ),
        Event::CredentialRenewed(event) => (Kind::CredentialRenewed, observe_credential(event)),
        Event::CredentialExpiring(event) => (Kind::CredentialExpiring, observe_credential(event)),
        Event::MailboxBackpressure(event) => (
            Kind::MailboxBackpressure,
            wire::Observation {
                queue_len: event.queue_len as u64,
                threshold: event.threshold as u64,
                ..Default::default()
            },
        ),
    };
    observation.kind = kind as i32;
    observation
}

fn observe_peer(event: PeerEvent) -> wire::Observation {
    wire::Observation {
        peer: Some(event.peer),
        peer_status: event.status.map(status_number),
        relayed: event.relayed,
        ..Default::default()
    }
}

fn observe_credential(event: actr_framework::CredentialEvent) -> wire::Observation {
    let duration = event
        .new_expiry
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    wire::Observation {
        expires_seconds: duration.as_secs(),
        expires_nanos: duration.subsec_nanos(),
        ..Default::default()
    }
}

fn status_number(status: WebRtcPeerStatus) -> u32 {
    match status {
        WebRtcPeerStatus::Idle => 0,
        WebRtcPeerStatus::Connecting => 1,
        WebRtcPeerStatus::Connected => 2,
        WebRtcPeerStatus::Recovering => 3,
    }
}

fn reason_number(reason: WebRtcUnavailableReason) -> Option<u32> {
    use WebRtcUnavailableReason as Reason;
    Some(match reason {
        Reason::ConnectionNotReady => 1,
        Reason::PeerConnectionBuildFailed => 2,
        Reason::NegotiationFailed => 3,
        Reason::IceRestartFailed => 4,
        Reason::PeerConnectionRebuildFailed => 5,
        Reason::UnsupportedRtcCapability => 6,
        Reason::RtcResourceLimitExceeded => 7,
        Reason::PeerUnavailable => 8,
        Reason::RecoveryExhausted => 9,
        Reason::Closed => 10,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::{EgressPermit, EgressRequest};
    use crate::oci::supervisor::GuestExit;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Policy(Arc<AtomicBool>);

    impl EgressPolicy for Policy {
        fn authorize(&self, _: EgressRequest<'_>) -> ActorResult<EgressPermit> {
            unreachable!("inbound routing must use the inbound policy hook")
        }

        fn authorize_inbound(
            &self,
            invocation: &InvocationContext,
            package: &str,
            privacy: &str,
            route: &str,
        ) -> ActorResult<()> {
            assert_eq!(invocation.caller_id, Some(actr_protocol::ActrId::default()));
            assert_eq!(package, "package-digest");
            assert_eq!(privacy, "privacy-digest");
            assert_eq!(route, "test.Echo.Ping");
            if self.0.load(Ordering::Acquire) {
                Ok(())
            } else {
                Err(ActrError::PermissionDenied("inbound grant revoked".into()))
            }
        }
    }

    #[test]
    fn signed_route_still_requires_current_host_authorization() {
        let allowed = Arc::new(AtomicBool::new(true));
        let admission = Admission {
            routes: BTreeSet::from(["test.Echo.Ping".into()]),
            inbound: true,
            policy: Some(InboundPolicy {
                policy: Arc::new(Policy(allowed.clone())),
                package_digest: "package-digest".into(),
                privacy_digest: "privacy-digest".into(),
            }),
            status: watch::channel(Status::Ready).0,
            failure: OnceLock::new(),
            memory_limit: 1024,
        };
        let invocation = InvocationContext {
            self_id: actr_protocol::ActrId::default(),
            caller_id: Some(actr_protocol::ActrId::default()),
            request_id: "request".into(),
        };

        assert!(admission.check("test.Echo.Ping").is_ok());
        assert!(
            admission
                .authorize_inbound(&invocation, "test.Echo.Ping")
                .is_ok()
        );
        allowed.store(false, Ordering::Release);
        assert!(matches!(
            admission.authorize_inbound(&invocation, "test.Echo.Ping"),
            Err(ActrError::PermissionDenied(_))
        ));
    }

    #[test]
    fn interrupted_dispatch_is_uncertain_even_without_oom() {
        use actr_protocol::Classify as _;

        for (error, failure, expected) in [
            (ActrError::TimedOut, GuestFailure::Disconnected, "timed out"),
            (
                unavailable("OCI Guest disconnected"),
                GuestFailure::Exited(GuestExit {
                    code: Some(1),
                    signal: None,
                    oom_killed: false,
                }),
                "exited with status 1",
            ),
            (
                unavailable("OCI Guest disconnected"),
                GuestFailure::Exited(GuestExit {
                    code: None,
                    signal: Some(9),
                    oom_killed: true,
                }),
                "killed by OOM",
            ),
        ] {
            let result = interrupted_invocation_error(
                wire::InvocationKind::Dispatch,
                "req",
                error,
                failure,
                1024,
            );
            assert!(!result.is_retryable());
            assert!(matches!(
                result,
                ActrError::DeliveryUncertain { operation_id, cause }
                    if operation_id == "req" && cause.contains(expected)
            ));
        }
    }
}
