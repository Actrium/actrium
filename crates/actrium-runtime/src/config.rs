use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use actrium_core::session::{ProgramRelease, Runtime};
use ed25519_dalek::SigningKey;

use crate::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentPolicy {
    Exclusive,
}

/// Exact, immutable Service compatibility and lease policy.
///
/// Compatibility is deliberately exact in v1: a request must select exactly
/// one configured runtime and the exact Program/package/Guest/protocol tuple.
pub struct ServiceConfig {
    pub service_actor_id: String,
    pub runtime_instance_id: String,
    pub service_signing_key: SigningKey,
    pub allowed_front_actor_types: BTreeSet<String>,
    pub program: ProgramRelease,
    pub package_digest: Vec<u8>,
    pub guest_digest: Vec<u8>,
    pub protocol_fingerprint: Vec<u8>,
    pub supported_runtimes: Vec<Runtime>,
    pub program_capabilities: BTreeSet<String>,
    /// Each route explicitly lists the session capabilities it accepts.
    pub routes: BTreeMap<String, BTreeSet<String>>,
    pub attachment_policy: AttachmentPolicy,
    pub lease_duration: Duration,
    pub heartbeat_interval: Duration,
    pub grace_period: Duration,
}

impl ServiceConfig {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.service_actor_id.is_empty() || self.runtime_instance_id.is_empty() {
            return Err(ConfigError(
                "Service and runtime identities are required".into(),
            ));
        }
        if self.allowed_front_actor_types.is_empty() {
            return Err(ConfigError(
                "at least one Front Actor type is required".into(),
            ));
        }
        if self.program.publisher.is_empty()
            || self.program.name.is_empty()
            || self.program.version.is_empty()
        {
            return Err(ConfigError("the exact Program Release is required".into()));
        }
        for (name, digest) in [
            ("package", &self.package_digest),
            ("Guest", &self.guest_digest),
            ("protocol", &self.protocol_fingerprint),
        ] {
            if digest.len() != 32 {
                return Err(ConfigError(format!("{name} digest must be SHA-256")));
            }
        }
        if self.supported_runtimes.is_empty() {
            return Err(ConfigError(
                "at least one runtime contract is required".into(),
            ));
        }
        for (index, left) in self.supported_runtimes.iter().enumerate() {
            if self.supported_runtimes[index + 1..]
                .iter()
                .any(|right| runtime_eq(left, right))
            {
                return Err(ConfigError("runtime contracts must be unique".into()));
            }
        }
        if self.routes.is_empty() || self.routes.keys().any(String::is_empty) {
            return Err(ConfigError(
                "explicit, non-empty routes are required".into(),
            ));
        }
        if self.lease_duration.is_zero()
            || self.heartbeat_interval.is_zero()
            || self.heartbeat_interval > self.lease_duration
        {
            return Err(ConfigError(
                "heartbeat must be positive and within the lease".into(),
            ));
        }
        if self.heartbeat_interval.as_millis() > u128::from(u32::MAX) {
            return Err(ConfigError(
                "heartbeat interval must fit the protocol's u32 milliseconds field".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn runtime_is_supported(&self, runtime: &Runtime) -> bool {
        self.supported_runtimes
            .iter()
            .filter(|candidate| runtime_eq(candidate, runtime))
            .count()
            == 1
    }

    pub(crate) fn program_matches(&self, program: &ProgramRelease) -> bool {
        self.program.publisher == program.publisher
            && self.program.name == program.name
            && self.program.version == program.version
    }

    pub(crate) fn program_string(&self) -> String {
        format!(
            "{}/{}@{}",
            self.program.publisher, self.program.name, self.program.version
        )
    }
}

pub(crate) fn runtime_eq(left: &Runtime, right: &Runtime) -> bool {
    left.actrium_version == right.actrium_version
        && left.front_host_api == right.front_host_api
        && left.keel_api == right.keel_api
}
