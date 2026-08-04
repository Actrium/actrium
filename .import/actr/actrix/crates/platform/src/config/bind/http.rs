use serde::{Deserialize, Serialize};

/// HTTP/HTTPS service binding configuration
///
/// Unified HTTP binding configuration. Configuring `cert` and `key` enables TLS (HTTPS),
/// otherwise it is plain HTTP mode.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HttpBindConfig {
    /// Domain name
    ///
    /// The domain name the service is bound to, used for generating correct URLs and certificate validation.
    pub domain_name: String,

    /// Public IP address
    ///
    /// The IP address advertised by the service to the public; clients will use this to connect.
    /// In NAT environments, this is typically the router's public IP.
    pub advertised_ip: String,

    /// Bind IP address
    ///
    /// The network interface IP address the service actually binds to.
    /// Defaults to "::" to cover both IPv4/IPv6 on dual-stack systems.
    pub ip: String,

    /// Bind port
    ///
    /// The port number the service listens on.
    pub port: u16,

    /// Public advertised port (optional)
    ///
    /// The port number advertised to the public. In NAT/reverse proxy environments, this may differ from the bind port.
    /// Defaults to the same as `port`.
    #[serde(default)]
    pub advertised_port: Option<u16>,

    /// TLS certificate file path (optional)
    ///
    /// Path to the PEM-format TLS certificate file. Configuring this field and `key` enables HTTPS.
    #[serde(default)]
    pub cert: Option<String>,

    /// TLS private key file path (optional)
    ///
    /// Path to the PEM-format private key file corresponding to the certificate.
    #[serde(default)]
    pub key: Option<String>,
}

impl HttpBindConfig {
    /// Whether TLS is enabled
    ///
    /// Returns true when both `cert` and `key` are configured with non-empty values.
    pub fn is_tls(&self) -> bool {
        self.cert.as_ref().is_some_and(|c| !c.is_empty())
            && self.key.as_ref().is_some_and(|k| !k.is_empty())
    }

    /// Returns the effective advertised port
    ///
    /// Prefers `advertised_port`; falls back to `port` if not configured.
    pub fn effective_advertised_port(&self) -> u16 {
        self.advertised_port.unwrap_or(self.port)
    }

    /// Returns the protocol scheme string
    pub fn scheme(&self) -> &'static str {
        if self.is_tls() { "https" } else { "http" }
    }

    /// Returns the WebSocket protocol scheme string
    pub fn ws_scheme(&self) -> &'static str {
        if self.is_tls() { "wss" } else { "ws" }
    }
}

impl Default for HttpBindConfig {
    fn default() -> Self {
        Self {
            domain_name: "localhost".to_string(),
            advertised_ip: "127.0.0.1".to_string(),
            ip: "::".to_string(),
            port: 8080,
            advertised_port: None,
            cert: None,
            key: None,
        }
    }
}
