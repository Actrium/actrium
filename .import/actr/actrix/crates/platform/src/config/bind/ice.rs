use serde::{Deserialize, Serialize};

/// ICE service binding configuration
///
/// UDP network configuration for STUN and TURN services.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct IceBindConfig {
    /// Bind IP address
    ///
    /// The network interface IP address the UDP service binds to.
    pub ip: String,

    /// Bind port
    ///
    /// The UDP port the STUN/TURN service listens on. Standard port is 3478.
    pub port: u16,

    /// Public IP address
    ///
    /// The public IP address advertised by the ICE service. Clients use this address for STUN/TURN communication.
    /// Must be a real routable public IP; cannot use "0.0.0.0".
    pub advertised_ip: String,

    /// Public port
    ///
    /// The port number advertised by the ICE service to the public, usually the same as the bind port.
    #[serde(default = "default_advertised_port")]
    pub advertised_port: u16,
}

fn default_advertised_port() -> u16 {
    3478
}

impl Default for IceBindConfig {
    fn default() -> Self {
        Self {
            ip: "0.0.0.0".to_string(),
            port: 3478,
            advertised_ip: "127.0.0.1".to_string(),
            advertised_port: 3478,
        }
    }
}
