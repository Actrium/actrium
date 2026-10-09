/// Reuse the test-only CA and localhost certificate from the TLS regression tests.
pub fn tls_options() -> actr_config::DirectWebSocketSecurity {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/direct-websocket");
    actr_config::DirectWebSocketSecurity {
        tls_cert: Some(dir.join("cert.pem")),
        tls_key: Some(dir.join("key.pem")),
        ca_cert: Some(dir.join("ca.pem")),
        ..Default::default()
    }
}

/// Choose a concrete loopback port so discovery advertises the listener endpoint.
pub fn listen_port() -> u16 {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("reserve a loopback port")
        .local_addr()
        .expect("read the loopback port")
        .port()
}
