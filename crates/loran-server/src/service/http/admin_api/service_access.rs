//! Access information projected from the same configuration used to mount the
//! listeners. Only fields suitable for the authenticated Admin UI belong here.
use platform::config::LoranConfig;
use serde_json::{Value, json};

fn host_port(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

pub(in crate::service::http) fn service_access(
    cfg: &LoranConfig,
    relay_endpoint: Option<&str>,
) -> Value {
    let ice = &cfg.bind.ice;
    let ice_address = host_port(&ice.advertised_ip, ice.advertised_port);
    let ice_listen = host_port(&ice.ip, ice.port);
    let local_issuer = cfg
        .services
        .issuer
        .as_ref()
        .filter(|_| cfg.is_issuer_enabled());
    let token_ttl = local_issuer.map(|issuer| issuer.server.token_ttl_secs);
    let turn_auth = if !cfg.turn.users.is_empty() {
        "static"
    } else if cfg.uses_realm_turn_auth() {
        "realm"
    } else {
        "rest"
    };
    let mut access = json!({
        "stun": {"url":format!("stun:{ice_address}"), "listen":ice_listen,
            "source":"bind.ice", "shared":cfg.is_turn_enabled()},
        "turn": {"url":format!("turn:{ice_address}?transport=udp"), "listen":ice_listen,
            "source":"bind.ice", "shared":cfg.is_stun_enabled(), "turn_auth":turn_auth,
            "issuer_ttl_secs":token_ttl.filter(|_| turn_auth == "realm")},
        "signer": {"url":cfg.signer_client_endpoint(),
            "listen":host_port(&cfg.signer_bind_ip(), cfg.signer_bind_port()),
            "source":if cfg.bind.signer.is_some() { "bind.signer" } else { "Default Signer listener" },
            "mtls":cfg.signer_listener_tls()}
    });
    if let Some(http) = &cfg.bind.http {
        let host = host_port(&http.domain_name, http.effective_advertised_port());
        let origin = format!("{}://{host}", http.scheme());
        let listen = host_port(&http.ip, http.port);
        access["signaling"] = json!({
            "url":format!("{}://{host}/signaling/ws", http.ws_scheme()), "listen":listen, "source":"bind.http"
        });
        access["issuer"] = json!({"url":format!("{origin}/issuer"), "listen":listen, "source":"bind.http",
            "issuer_ttl_secs":token_ttl, "renewal_ttl_secs":local_issuer.map(|i| i.server.renewal_token_ttl_secs),
            "relay_identity_max_secs": actr_relay_security::CERTIFICATE_TTL});
        access["publishers"] = json!({"url":format!("{origin}/admin/api/publishers/admin/list"), "listen":listen, "source":"bind.http"});
        access["relay"] = json!({"url":relay_endpoint, "listen":listen,
            "source":if cfg.relay.public_url.is_some() { "relay.public_url" } else { "bind.http" }});
    }
    access
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_distinguishes_advertised_and_bind_addresses() {
        let mut cfg = LoranConfig::default();
        let http = cfg.bind.http.as_mut().unwrap();
        http.ip = "0.0.0.0".into();
        http.domain_name = "node.example".into();
        http.port = 9000;
        http.advertised_port = Some(443);
        http.cert = Some("cert".into());
        http.key = Some("key".into());
        cfg.bind.ice.ip = "::".into();
        cfg.bind.ice.port = 5000;
        cfg.bind.ice.advertised_ip = "2001:db8::5".into();
        cfg.bind.ice.advertised_port = 6000;
        cfg.relay.public_url = Some("wss://relay.example/channels".into());
        cfg.bind.signer = None;
        let info = service_access(&cfg, cfg.relay.public_url.as_deref());
        assert_eq!(
            info["signaling"]["url"],
            "wss://node.example:443/signaling/ws"
        );
        assert_eq!(info["stun"]["listen"], "[::]:5000");
        assert_eq!(info["turn"]["url"], "turn:[2001:db8::5]:6000?transport=udp");
        assert_eq!(info["relay"]["url"], "wss://relay.example/channels");
        assert_eq!(info["relay"]["listen"], "0.0.0.0:9000");
        assert_eq!(info["issuer"]["url"], "https://node.example:443/issuer");
        assert_eq!(info["signer"]["listen"], "127.0.0.1:9001");
        assert_eq!(
            info["publishers"]["url"],
            "https://node.example:443/admin/api/publishers/admin/list"
        );
    }
}
