//! TLS and endpoint policy for peer direct WebSocket; signaling uses its own policy.
use crate::error::{HyperError, HyperResult};
use crate::transport::{NetworkError, NetworkResult};
use actr_config::{DirectWebSocketSecurity, RuntimeConfig, Url};
use std::io::BufReader;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tokio_rustls::rustls::{self, ClientConfig, RootCertStore, ServerConfig};

#[derive(Clone, Debug, Default)]
pub(crate) struct DirectSecurity {
    pub(crate) server: Option<Arc<ServerConfig>>,
    pub(crate) client: Option<Arc<ClientConfig>>,
    pub(crate) allow_insecure: bool,
    pub(crate) bind_address: Option<IpAddr>,
}

fn config_error(message: impl Into<String>) -> HyperError {
    HyperError::Config(message.into())
}

/// Client trust roots for outbound direct connections.
///
/// A configured `ca_cert` replaces the defaults and is re-read each call so a
/// bad path fails loudly. The default (WebPKI + native store) is cached for
/// the process lifetime: loading the native store is blocking I/O and node
/// startup calls this even when no direct listener is configured.
fn client_roots(options: &DirectWebSocketSecurity) -> HyperResult<Arc<RootCertStore>> {
    if let Some(path) = &options.ca_cert {
        let mut roots = RootCertStore::empty();
        for cert in certificates(path)? {
            roots
                .add(cert)
                .map_err(|_| config_error("invalid direct WebSocket CA certificate"))?;
        }
        return Ok(Arc::new(roots));
    }
    static DEFAULT_ROOTS: OnceLock<Arc<RootCertStore>> = OnceLock::new();
    Ok(DEFAULT_ROOTS
        .get_or_init(|| {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
            Arc::new(roots)
        })
        .clone())
}

fn certificates(path: &Path) -> HyperResult<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let file = std::fs::File::open(path).map_err(|e| {
        config_error(format!(
            "cannot open direct WebSocket certificate {}: {e}",
            path.display()
        ))
    })?;
    let certs = rustls_pemfile::certs(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| config_error("invalid direct WebSocket PEM certificate"))?;
    if certs.is_empty() {
        return Err(config_error(
            "direct WebSocket certificate file contains no certificates",
        ));
    }
    Ok(certs)
}

impl DirectSecurity {
    /// Load and validate before registration can advertise this endpoint.
    pub(crate) fn load(config: &RuntimeConfig) -> HyperResult<Self> {
        Self::from_options(&config.websocket_security, config.websocket_listen_port)
    }

    pub(crate) fn from_options(
        options: &DirectWebSocketSecurity,
        port: Option<u16>,
    ) -> HyperResult<Self> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let server = match (&options.tls_cert, &options.tls_key) {
            (Some(cert), Some(key)) => {
                let chain = certificates(cert)?;
                let file = std::fs::File::open(key).map_err(|e| {
                    config_error(format!(
                        "cannot open direct WebSocket private key {}: {e}",
                        key.display()
                    ))
                })?;
                let key = rustls_pemfile::private_key(&mut BufReader::new(file))
                    .map_err(|_| config_error("invalid direct WebSocket PEM private key"))?
                    .ok_or_else(|| {
                        config_error("direct WebSocket private key file contains no key")
                    })?;
                Some(Arc::new(
                    ServerConfig::builder_with_provider(provider.clone())
                        .with_safe_default_protocol_versions()
                        .map_err(|_| config_error("invalid TLS protocol configuration"))?
                        .with_no_client_auth()
                        .with_single_cert(chain, key)
                        .map_err(|_| {
                            config_error("invalid direct WebSocket certificate/key pair")
                        })?,
                ))
            }
            (None, None) => None,
            _ => {
                return Err(config_error(
                    "direct WebSocket tls_cert and tls_key must be configured together",
                ));
            }
        };
        if port.is_some()
            && server.is_none()
            && (!options.allow_insecure || !options.bind_address.is_loopback())
        {
            return Err(config_error(
                "direct WebSocket requires TLS; allow_insecure permits only a loopback listener",
            ));
        }
        let client = Some(Arc::new(
            ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(|_| config_error("invalid TLS protocol configuration"))?
                .with_root_certificates(client_roots(options)?)
                .with_no_client_auth(),
        ));
        Ok(Self {
            server,
            client,
            allow_insecure: options.allow_insecure,
            bind_address: Some(options.bind_address),
        })
    }

    pub(crate) fn listen_address(&self, port: u16) -> SocketAddr {
        SocketAddr::new(
            self.bind_address
                .unwrap_or_else(|| std::net::Ipv4Addr::LOCALHOST.into()),
            port,
        )
    }

    pub(crate) fn advertised_url(&self, config: &RuntimeConfig) -> HyperResult<Option<String>> {
        let Some(port) = config.websocket_listen_port else {
            return Ok(None);
        };
        self.advertise(config.websocket_advertised_host.as_deref(), port)
    }

    /// Advertise the actually bound port, so an ephemeral `listen_port = 0`
    /// registers a reachable endpoint instead of an unusable port-zero address.
    pub(crate) fn advertised_url_for(
        &self,
        config: &RuntimeConfig,
        port: u16,
    ) -> HyperResult<Option<String>> {
        if config.websocket_listen_port.is_none() {
            return Ok(None);
        }
        self.advertise(config.websocket_advertised_host.as_deref(), port)
    }

    fn advertise(
        &self,
        advertised_host: Option<&str>,
        port: u16,
    ) -> HyperResult<Option<String>> {
        if port == 0 {
            return Err(config_error(
                "direct WebSocket cannot advertise port 0; advertise the bound port",
            ));
        }
        let default_host = self.listen_address(port).ip().to_string();
        let host = advertised_host.unwrap_or(&default_host);
        let host = if host.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        // Parse the host before the guards so alternate spellings (e.g. "0"
        // for 0.0.0.0) cannot normalize into a wildcard after the check.
        let host = url::Host::parse(&host)
            .map_err(|_| config_error("invalid direct WebSocket advertised_host"))?;
        let advertised_ip = match &host {
            url::Host::Ipv4(ip) => Some(IpAddr::V4(*ip)),
            url::Host::Ipv6(ip) => Some(IpAddr::V6(*ip)),
            url::Host::Domain(_) => None,
        };
        if advertised_ip.is_some_and(|ip| ip.is_unspecified()) {
            return Err(config_error(
                "wildcard direct WebSocket binds require advertised_host",
            ));
        }
        // A routable advertised endpoint over the loopback bind registers an
        // address no peer can reach; fail at registration instead. (Before
        // bind_address existed, such a config bound the wildcard and worked.)
        let bind_ip = self.bind_address.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        if bind_ip.is_loopback() {
            let reachable_from_loopback = match &host {
                url::Host::Ipv4(ip) => ip.is_loopback(),
                url::Host::Ipv6(ip) => ip.is_loopback(),
                // Never resolve a name here; "localhost" is the only name
                // guaranteed to be loopback.
                url::Host::Domain(domain) => domain
                    .trim_end_matches('.')
                    .eq_ignore_ascii_case("localhost"),
            };
            if !reachable_from_loopback {
                return Err(config_error(
                    "direct WebSocket advertised_host is not reachable from the loopback bind_address; set bind_address to an interface that serves it",
                ));
            }
        }
        let scheme = if self.server.is_some() { "wss" } else { "ws" };
        let url = format!("{scheme}://{host}:{port}");
        self.validate_url(&url)
            .map_err(|e| config_error(e.to_string()))?;
        Ok(Some(url))
    }

    pub(crate) fn validate_url(&self, address: &str) -> NetworkResult<()> {
        let invalid = || {
            NetworkError::InvalidArgument("direct WebSocket requires wss:// (development ws:// requires allow_insecure and a literal loopback IP)".into())
        };
        let url = Url::parse(address).map_err(|_| invalid())?;
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid());
        }
        match url.scheme() {
            "wss" => Ok(()),
            "ws" if self.allow_insecure => {
                // Never resolve a hostname for the plaintext exception: avoids DNS rebinding.
                let host = url
                    .host_str()
                    .unwrap()
                    .trim_start_matches('[')
                    .trim_end_matches(']');
                if host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()) {
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
            _ => Err(invalid()),
        }
    }

    #[cfg(test)]
    pub(crate) fn insecure_loopback() -> Self {
        Self {
            allow_insecure: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
pub(crate) fn test_options() -> DirectWebSocketSecurity {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/direct-websocket");
    DirectWebSocketSecurity {
        tls_cert: Some(dir.join("cert.pem")),
        tls_key: Some(dir.join("key.pem")),
        ca_cert: Some(dir.join("ca.pem")),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_requires_explicit_literal_loopback_policy() {
        let secure = DirectSecurity::default();
        let dev = DirectSecurity::insecure_loopback();
        for url in ["ws://127.0.0.1:80", "ws://[::1]:80"] {
            assert!(secure.validate_url(url).is_err());
            assert!(dev.validate_url(url).is_ok());
        }
        for url in [
            "ws://localhost:80",
            "ws://192.0.2.1:80",
            "ws://0.0.0.0:80",
            "ws://[::]:80",
            "https://example.com",
            "wss://user:secret@example.com",
            "wss://example.com/#fragment",
        ] {
            assert!(secure.validate_url(url).is_err(), "{url}");
            assert!(dev.validate_url(url).is_err(), "{url}");
        }
        assert!(secure.validate_url("wss://example.com:443").is_ok());
    }

    #[test]
    fn listener_fails_closed_and_defaults_to_loopback() {
        let mut options = DirectWebSocketSecurity::default();
        assert!(options.bind_address.is_loopback());
        assert!(DirectSecurity::from_options(&options, None).is_ok());
        assert!(DirectSecurity::from_options(&options, Some(9000)).is_err());
        options.allow_insecure = true;
        // Ephemeral ports are allowed here; the bound port is advertised.
        assert!(DirectSecurity::from_options(&options, Some(0)).is_ok());
        assert!(DirectSecurity::from_options(&options, Some(9000)).is_ok());
        for bind in ["0.0.0.0", "::", "192.0.2.1"] {
            options.bind_address = bind.parse().unwrap();
            assert!(DirectSecurity::from_options(&options, Some(9000)).is_err());
        }
    }

    #[test]
    fn advertised_host_guards_parse_before_rejecting_wildcards() {
        let options = DirectWebSocketSecurity {
            allow_insecure: true,
            ..Default::default()
        };
        let security = DirectSecurity::from_options(&options, Some(9000)).unwrap();
        // Alternate wildcard spellings must not bypass the guard.
        for host in [
            "0",
            "0.0.0.0",
            "0.0.0.0.",
            "::",
            "[::]",
            "0:0:0:0:0:0:0:0",
            "[0:0:0:0:0:0:0:0]",
        ] {
            assert!(security.advertise(Some(host), 9000).is_err(), "{host}");
        }
        // A routable advertised host over the loopback bind would register an
        // endpoint no peer can reach.
        for host in ["192.0.2.1", "peer.example.com"] {
            assert!(security.advertise(Some(host), 9000).is_err(), "{host}");
        }
        // Loopback literals stay reachable over the loopback default bind.
        for (host, expected) in [
            (Some("127.0.0.1"), "ws://127.0.0.1:9000"),
            (Some("::1"), "ws://[::1]:9000"),
            (None, "ws://127.0.0.1:9000"),
        ] {
            assert_eq!(
                security.advertise(host, 9000).unwrap().as_deref(),
                Some(expected),
                "{host:?}"
            );
        }
        // Port zero cannot be advertised; nodes advertise the bound port.
        assert!(security.advertise(None, 0).is_err());

        // With TLS the same guards hold; "localhost" names the loopback and
        // only a non-loopback bind may advertise a routable host.
        let tls = DirectSecurity::from_options(&test_options(), Some(9000)).unwrap();
        assert_eq!(
            tls.advertise(Some("localhost"), 9000).unwrap().as_deref(),
            Some("wss://localhost:9000")
        );
        assert!(tls.advertise(Some("peer.example.com"), 9000).is_err());
        let mut exposed_options = test_options();
        exposed_options.bind_address = "0.0.0.0".parse().unwrap();
        let exposed = DirectSecurity::from_options(&exposed_options, Some(9000)).unwrap();
        assert_eq!(
            exposed
                .advertise(Some("peer.example.com"), 9000)
                .unwrap()
                .as_deref(),
            Some("wss://peer.example.com:9000")
        );
    }

    #[test]
    fn tls_material_is_validated_even_without_a_listener() {
        let options = test_options();
        assert!(DirectSecurity::from_options(&options, Some(9000)).is_ok());
        let mut invalid = options.clone();
        invalid.tls_key = None;
        assert!(DirectSecurity::from_options(&invalid, None).is_err());
        let mut invalid = options.clone();
        invalid.tls_cert = None;
        assert!(DirectSecurity::from_options(&invalid, None).is_err());
        for field in ["cert", "key", "ca"] {
            let mut invalid = options.clone();
            let path = Some(std::path::PathBuf::from("/nonexistent/direct-ws.pem"));
            match field {
                "cert" => invalid.tls_cert = path,
                "key" => invalid.tls_key = path,
                _ => invalid.ca_cert = path,
            }
            assert!(DirectSecurity::from_options(&invalid, None).is_err());
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "not PEM").unwrap();
        let mut invalid = options;
        invalid.ca_cert = Some(file.path().to_owned());
        assert!(DirectSecurity::from_options(&invalid, None).is_err());
    }
}
