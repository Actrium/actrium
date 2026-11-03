//! WebSocket relay configuration and deployment-specific authentication.

use loran_relay_protocol::{DATA_HEADER_LEN, RelayLimits, valid_endpoint};
use serde::{Deserialize, Serialize};

// One maximum-sized SDK Noise record plus the relay's channel identifier.
// The server integration test checks this against the security crate's limit.
const MIN_SDK_FRAME_BYTES: usize = 65535 + 13 + DATA_HEADER_LEN;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RelayConfig {
    /// Advertised WSS endpoint. Omit for the local /relay/ws endpoint. This
    /// may also advertise a separate relay when the local service is disabled.
    pub public_url: Option<String>,
    pub auth: RelayAuthMode,
    pub static_credentials: Vec<RelayStaticCredential>,
    pub limits: RelayLimits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelayAuthMode {
    /// Use AIS-signed, relay-scoped identity certificates, not signaling bearers.
    #[default]
    Aid,
    /// Independent deployments with externally provisioned endpoint tokens.
    Static,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayStaticCredential {
    pub endpoint: String,
    pub namespace: String,
    pub token: String,
}

impl std::fmt::Debug for RelayStaticCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayStaticCredential")
            .field("endpoint", &self.endpoint)
            .field("namespace", &self.namespace)
            .field("token", &"[redacted]")
            .finish()
    }
}

impl RelayConfig {
    pub fn validate(&self, enabled: bool, production: bool) -> Result<(), String> {
        if let Some(url) = &self.public_url {
            let url = url::Url::parse(url)
                .map_err(|_| "relay.public_url must be an absolute WebSocket URL")?;
            if !matches!(url.scheme(), "ws" | "wss")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || url.query().is_some()
                || (production && url.scheme() != "wss")
            {
                return Err("relay.public_url must be a WSS URL without credentials, query or fragment (WS allowed outside production)".into());
            }
        }
        if !enabled {
            return Ok(());
        }
        self.limits.validate().map_err(str::to_owned)?;
        if self.limits.max_frame_bytes < MIN_SDK_FRAME_BYTES {
            return Err(format!(
                "relay.limits.max_frame_bytes must be at least {MIN_SDK_FRAME_BYTES} for SDK frames"
            ));
        }
        if self.limits.max_connections > tokio::sync::Semaphore::MAX_PERMITS
            || self.limits.queue_messages > tokio::sync::Semaphore::MAX_PERMITS
            || self.limits.queue_bytes as usize > tokio::sync::Semaphore::MAX_PERMITS
        {
            return Err("relay queue or connection limit exceeds semaphore capacity".into());
        }
        if self.auth == RelayAuthMode::Static {
            if self.static_credentials.is_empty() {
                return Err("relay static authentication requires endpoint credentials".into());
            }
            let mut endpoints = std::collections::HashSet::new();
            for credential in &self.static_credentials {
                if !valid_endpoint(&credential.endpoint)
                    || credential.namespace.is_empty()
                    || credential.namespace.len() > 512
                    || credential.token.len() < 32
                    || !endpoints.insert(credential.endpoint.clone())
                {
                    return Err("relay static credentials require unique valid endpoints, namespaces and tokens of at least 32 bytes".into());
                }
            }
        } else if !self.static_credentials.is_empty() {
            return Err("relay.static_credentials requires relay.auth = 'static'".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_semaphore_limits_are_rejected_during_configuration_validation() {
        let mut config = RelayConfig::default();
        config.limits.max_connections = tokio::sync::Semaphore::MAX_PERMITS + 1;
        assert!(
            config
                .validate(true, false)
                .unwrap_err()
                .contains("capacity")
        );
        config.limits.max_connections = 1;
        config.limits.queue_messages = tokio::sync::Semaphore::MAX_PERMITS + 1;
        assert!(
            config
                .validate(true, false)
                .unwrap_err()
                .contains("capacity")
        );
    }
}
