use serde::{Deserialize, Serialize};

/// TURN service configuration
///
/// Dedicated configuration parameters for the TURN relay service.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TurnConfig {
    /// Relay port range
    ///
    /// UDP port range used by the TURN service for data relay.
    /// Format: start_port-end_port, e.g. "49152-65535".
    /// The larger the range, the more concurrent relay sessions can be supported.
    pub relay_port_range: String,

    /// TURN authentication realm
    ///
    /// The authentication realm for the TURN service, used in the TURN protocol authentication mechanism.
    pub realm: String,

    /// TURN shared secret (HMAC time-limited credentials)
    ///
    /// Used to generate and verify TURN time-limited credentials (coturn --use-auth-secret compatible format).
    /// AIS uses this secret when generating TurnCredentials; the TURN server uses the same secret for verification.
    #[serde(default = "default_turn_secret")]
    pub turn_secret: String,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            relay_port_range: "49152-65535".to_string(),
            realm: "actrix.local".to_string(),
            turn_secret: default_turn_secret(),
        }
    }
}

fn default_turn_secret() -> String {
    "actrix-turn-secret-change-in-production".to_string()
}
