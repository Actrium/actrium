use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// TURN service configuration
///
/// Dedicated configuration parameters for the TURN relay service.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TurnConfig {
    /// Relay port range
    ///
    /// The UDP port range used by the TURN service for data relay.
    /// Format: start_port-end_port, e.g. "49152-65535".
    /// A larger range supports more concurrent relay sessions.
    pub relay_port_range: String,

    /// TURN authentication realm
    ///
    /// The authentication domain name for the TURN service, used in the TURN protocol authentication mechanism.
    pub realm: String,

    /// TURN shared secret (HMAC time-limited credentials)
    ///
    /// Used to generate and verify TURN time-limited credentials (coturn --use-auth-secret compatible format).
    /// Issuer uses this secret to generate TurnCredential; the TURN server uses the same secret for verification.
    #[serde(default)]
    pub turn_secret: String,

    /// Static long-term credentials. When nonempty, replaces shared-secret mode.
    #[serde(default)]
    pub users: BTreeMap<String, String>,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            relay_port_range: "49152-65535".to_string(),
            realm: "loran.local".to_string(),
            turn_secret: String::new(),
            users: BTreeMap::new(),
        }
    }
}

impl TurnConfig {
    pub fn relay_port_bounds(&self) -> Result<(u16, u16), String> {
        let (start, end) = self
            .relay_port_range
            .split_once('-')
            .ok_or_else(|| "turn.relay_port_range must use start-end syntax".to_string())?;
        let start = start
            .trim()
            .parse::<u16>()
            .map_err(|_| "turn.relay_port_range start must be a valid port".to_string())?;
        let end = end
            .trim()
            .parse::<u16>()
            .map_err(|_| "turn.relay_port_range end must be a valid port".to_string())?;

        if start < 1024 {
            return Err("turn.relay_port_range must not include privileged ports".to_string());
        }
        if start > end {
            return Err("turn.relay_port_range start must not exceed end".to_string());
        }

        Ok((start, end))
    }
}

impl TurnConfig {
    /// Parse the inclusive relay port range before binding any sockets.
    pub fn relay_ports(&self) -> Result<(u16, u16), String> {
        self.relay_port_bounds()
    }
}

#[cfg(test)]
mod tests {
    use super::TurnConfig;

    #[test]
    fn parses_relay_port_range() {
        let config = TurnConfig {
            relay_port_range: "50000-50100".to_string(),
            ..TurnConfig::default()
        };
        assert_eq!(config.relay_port_bounds().unwrap(), (50_000, 50_100));
    }

    #[test]
    fn rejects_invalid_relay_port_ranges() {
        for value in ["50000", "60000-50000", "100-200", "x-50000"] {
            let config = TurnConfig {
                relay_port_range: value.to_string(),
                ..TurnConfig::default()
            };
            assert!(config.relay_port_bounds().is_err(), "{value} should fail");
        }
    }
}
