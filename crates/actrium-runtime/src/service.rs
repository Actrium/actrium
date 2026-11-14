use std::sync::Arc;

use actrium_core::session::{
    AttachRequest, DetachRequest, FrontServiceSession, HeartbeatRequest, ResumeRequest,
    SessionStatus, TakeoverRequest, VerifiedChannel,
};

use crate::{
    AttachOutcome, Clock, ConfigError, HeartbeatOutcome, InMemorySessionRegistry, ServiceConfig,
    ServicePolicies, SessionError, SessionGuard,
};

/// Public Service Host entry point. These are protocol operations, never
/// `on_start`/`on_stop` workload callbacks.
#[derive(Clone)]
pub struct InMemorySessionService {
    registry: InMemorySessionRegistry,
}

impl InMemorySessionService {
    pub fn new(config: ServiceConfig, policies: ServicePolicies) -> Result<Self, ConfigError> {
        Ok(Self {
            registry: InMemorySessionRegistry::new(config, policies)?,
        })
    }

    pub fn new_with_clock(
        config: ServiceConfig,
        policies: ServicePolicies,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            registry: InMemorySessionRegistry::new_with_clock(config, policies, clock)?,
        })
    }

    pub fn from_registry(registry: InMemorySessionRegistry) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> &InMemorySessionRegistry {
        &self.registry
    }

    pub fn guard(&self) -> SessionGuard {
        SessionGuard::new(self.registry.clone())
    }

    pub async fn attach(
        &self,
        request: AttachRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, SessionError> {
        self.registry.attach(request, channel).await
    }

    pub async fn takeover(
        &self,
        takeover: TakeoverRequest,
        request: AttachRequest,
        channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, SessionError> {
        self.registry.takeover(takeover, request, channel).await
    }

    pub async fn heartbeat(
        &self,
        request: HeartbeatRequest,
        channel: &VerifiedChannel,
        renewed_channel: Option<&VerifiedChannel>,
    ) -> Result<HeartbeatOutcome, SessionError> {
        self.registry
            .heartbeat(request, channel, renewed_channel)
            .await
    }

    pub async fn resume(
        &self,
        request: ResumeRequest,
        new_channel: &VerifiedChannel,
    ) -> Result<AttachOutcome, SessionError> {
        self.registry.resume(request, new_channel).await
    }

    pub async fn detach(
        &self,
        request: DetachRequest,
        channel: &VerifiedChannel,
    ) -> Result<(), SessionError> {
        self.registry.detach(request, channel).await
    }

    pub async fn handle_disconnect(
        &self,
        session_id: &str,
        epoch: u64,
    ) -> Result<(), SessionError> {
        self.registry.handle_disconnect(session_id, epoch).await
    }

    pub async fn expire_grace(&self, session_id: &str) -> Result<bool, SessionError> {
        self.registry.expire_grace(session_id).await
    }

    pub async fn invalidate_all(&self) {
        self.registry.invalidate_all().await
    }

    pub async fn restart(
        &self,
        new_runtime_instance_id: impl Into<String>,
    ) -> Result<(), SessionError> {
        self.registry.restart(new_runtime_instance_id).await
    }

    pub async fn status(&self, session_id: &str) -> Result<SessionStatus, SessionError> {
        self.registry.status(session_id).await
    }

    pub async fn session(&self, session_id: &str) -> Option<FrontServiceSession> {
        self.registry.session(session_id).await
    }
}
