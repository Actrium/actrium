//! Exercise relay and relay+ICE deployments through the regular Loran launcher.
use futures::{SinkExt, StreamExt};
use loran_relay_protocol::{
    ClientControl, RelayDiscovery, SUBPROTOCOL, ServerControl, encode_data,
};
use std::{
    process::{Child, Command},
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    Connector, connect_async_tls_with_config,
    tungstenite::{Message, client::IntoClientRequest},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn loran_frame_limit_fits_the_largest_sdk_record() {
    let mut config = loran_server::LoranConfig::from_toml(include_str!(
        "../../../loran/config/relay.example.toml"
    ))
    .unwrap();
    let required = actr_relay_security::MAX_RECORD_BYTES + loran_relay_protocol::DATA_HEADER_LEN;
    config.relay.limits.max_frame_bytes = required - 1;
    assert!(config.relay.validate(true, false).is_err());
    config.relay.limits.max_frame_bytes = required;
    assert!(config.relay.validate(true, false).is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn static_relay_can_be_disabled_without_database_on_reload() {
    let directory = tempfile::tempdir().unwrap();
    let storage = directory.path().join("storage-must-stay-a-file");
    std::fs::write(&storage, b"untouched").unwrap();
    let mut config = loran_server::LoranConfig::from_toml(include_str!(
        "../../../loran/config/relay.example.toml"
    ))
    .unwrap();
    config.sqlite_path = storage.clone();
    let port = tcp_port();
    config.bind.http.as_mut().unwrap().port = port;
    let path = directory.path().join("relay.toml");
    std::fs::write(&path, config.to_toml().unwrap()).unwrap();
    let log_path = directory.path().join("server.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_loran"))
            .arg("--config")
            .arg(&path)
            .current_dir(directory.path())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let url = format!("http://127.0.0.1:{port}/relay/config");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(process.0.try_wait().unwrap().is_none());
        if let Ok(response) = http.get(&url).send().await
            && response.json::<RelayDiscovery>().await.unwrap().enabled
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{}",
            std::fs::read_to_string(&log_path).unwrap()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    config.enable = 0;
    std::fs::write(&path, config.to_toml().unwrap()).unwrap();
    assert!(
        Command::new("kill")
            .args(["-HUP", &process.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(process.0.try_wait().unwrap().is_none());
        if let Ok(response) = http.get(&url).send().await
            && !response.json::<RelayDiscovery>().await.unwrap().enabled
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{}",
            std::fs::read_to_string(&log_path).unwrap()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(std::fs::read(storage).unwrap(), b"untouched");
}

#[tokio::test]
async fn regular_launcher_serves_static_relay_without_database_alone_or_with_ice() {
    tokio::time::timeout(Duration::from_secs(60), exercise_regular_launcher())
        .await
        .expect("relay deployment deadline exceeded");
}

async fn exercise_regular_launcher() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    for (enable, tls) in [(32, false), (38, false), (32, true)] {
        let directory = tempfile::tempdir().unwrap();
        let storage = directory.path().join("storage-must-stay-a-file");
        std::fs::write(&storage, b"untouched").unwrap();
        let mut config = loran_server::LoranConfig::from_toml(include_str!(
            "../../../loran/config/relay.example.toml"
        ))
        .unwrap();
        config.enable = enable;
        config.sqlite_path = storage.clone();
        let port = tcp_port();
        config.bind.http.as_mut().unwrap().port = port;
        let certificate = tls.then(|| {
            let key = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
            let cert_path = directory.path().join("cert.pem");
            let key_path = directory.path().join("key.pem");
            std::fs::write(&cert_path, key.cert.pem()).unwrap();
            std::fs::write(&key_path, key.signing_key.serialize_pem()).unwrap();
            config.env = "prod".into();
            config.monitoring.allowed_ips = vec!["127.0.0.1".into()];
            let binding = config.bind.http.as_mut().unwrap();
            binding.cert = Some(cert_path.to_str().unwrap().into());
            binding.key = Some(key_path.to_str().unwrap().into());
            key.cert
        });
        if enable == 38 {
            let ice = loran_server::LoranConfig::from_toml(include_str!(
                "../../../loran/config/ice.example.toml"
            ))
            .unwrap();
            config.bind.ice = ice.bind.ice;
            config.turn = ice.turn;
            config.bind.ice.ip = "127.0.0.1".into();
            config.bind.ice.advertised_ip = "127.0.0.1".into();
            config.bind.ice.port = std::net::UdpSocket::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
        }
        let path = directory.path().join("relay.toml");
        std::fs::write(&path, config.to_toml().unwrap()).unwrap();
        let log_path = directory.path().join("server.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let mut process = Process(
            Command::new(env!("CARGO_BIN_EXE_loran"))
                .arg("--config")
                .arg(&path)
                .current_dir(directory.path())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let mut http = reqwest::Client::builder().timeout(Duration::from_secs(1));
        let connector = if let Some(certificate) = certificate.as_ref() {
            http = http
                .add_root_certificate(reqwest::Certificate::from_der(certificate.der()).unwrap());
            let mut roots = rustls::RootCertStore::empty();
            roots.add(certificate.der().clone()).unwrap();
            Some(Connector::Rustls(std::sync::Arc::new(
                rustls::ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )))
        } else {
            None
        };
        let http = http.build().unwrap();
        let scheme = if tls { "https" } else { "http" };
        let ws_scheme = if tls { "wss" } else { "ws" };
        let origin = format!("{scheme}://127.0.0.1:{port}");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "{}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            if let Ok(response) = http.get(format!("{origin}/health")).send().await {
                let health: serde_json::Value = response.json().await.unwrap();
                if health["status"] == "healthy" {
                    assert_eq!(health["services"]["relay"], "healthy");
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "startup deadline: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let response = http
            .get(format!("{origin}/relay/config"))
            .header(reqwest::header::ORIGIN, "https://app.example.com")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.headers()[reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "*"
        );
        let discovery: RelayDiscovery = response.json().await.unwrap();
        assert!(discovery.enabled);
        assert_eq!(discovery.version, 1);
        let mut sockets = Vec::new();
        for credential in &config.relay.static_credentials {
            let mut request = format!("{ws_scheme}://127.0.0.1:{port}/relay/ws")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
            let (mut socket, _) =
                connect_async_tls_with_config(request, None, false, connector.clone())
                    .await
                    .unwrap();
            socket
                .send(Message::Text(
                    serde_json::to_string(&ClientControl::Hello {
                        endpoint: credential.endpoint.clone(),
                        token: credential.token.clone(),
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
            let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
                panic!()
            };
            assert!(matches!(
                serde_json::from_str::<ServerControl>(&text).unwrap(),
                ServerControl::Welcome { .. }
            ));
            sockets.push(socket);
        }
        sockets[0]
            .send(Message::Text(
                serde_json::to_string(&ClientControl::Connect {
                    request: 1,
                    peer: "endpoint-b".into(),
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
        let Message::Text(text) = sockets[1].next().await.unwrap().unwrap() else {
            panic!()
        };
        let ServerControl::Incoming { channel, .. } = serde_json::from_str(&text).unwrap() else {
            panic!()
        };
        sockets[1]
            .send(Message::Text(
                serde_json::to_string(&ClientControl::Accept { channel })
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
        for socket in &mut sockets {
            let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
                panic!()
            };
            assert!(matches!(
                serde_json::from_str::<ServerControl>(&text).unwrap(),
                ServerControl::Connected { .. }
            ));
        }
        let bytes = encode_data(channel, b"generic bytes without application identities").unwrap();
        sockets[0]
            .send(Message::Binary(bytes.clone()))
            .await
            .unwrap();
        assert_eq!(
            sockets[1].next().await.unwrap().unwrap(),
            Message::Binary(bytes)
        );
        assert_eq!(std::fs::read(&storage).unwrap(), b"untouched");
        #[cfg(unix)]
        {
            assert!(
                Command::new("kill")
                    .args(["-TERM", &process.0.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
            for socket in &mut sockets {
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        match socket.next().await.unwrap().unwrap() {
                            Message::Close(_) => break,
                            Message::Text(text) => assert!(matches!(
                                serde_json::from_str::<ServerControl>(&text).unwrap(),
                                ServerControl::Closed { .. }
                            )),
                            message => panic!("unexpected shutdown frame: {message:?}"),
                        }
                    }
                })
                .await
                .expect("relay sockets close on shutdown");
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = process.0.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                assert!(Instant::now() < deadline, "graceful shutdown deadline");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rejected_reloads_keep_existing_relay_sessions_and_accept_new_connections() {
    tokio::time::timeout(Duration::from_secs(45), exercise_rejected_reloads())
        .await
        .expect("reload regression deadline exceeded");
}

#[cfg(unix)]
async fn exercise_rejected_reloads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("relay.toml");
    let log_path = directory.path().join("server.log");
    let mut config = loran_server::LoranConfig::from_toml(include_str!(
        "../../../loran/config/relay.example.toml"
    ))
    .unwrap();
    config.sqlite_path = directory.path().join("unused-storage");
    config.loran_shared_key = "a-valid-preconfigured-shared-secret".into();
    let port = tcp_port();
    config.bind.http.as_mut().unwrap().port = port;
    std::fs::write(&path, config.to_toml().unwrap()).unwrap();
    let log = std::fs::File::create(&log_path).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_loran"))
            .arg("--config")
            .arg(&path)
            .current_dir(directory.path())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(process.0.try_wait().unwrap().is_none());
        if http
            .get(format!("http://127.0.0.1:{port}/relay/config"))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        assert!(Instant::now() < deadline, "startup deadline");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let connect = || async {
        let mut request = format!("ws://127.0.0.1:{port}/relay/ws")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
        connect_async_tls_with_config(request, None, false, None)
            .await
            .unwrap()
            .0
    };
    let mut original = connect().await;
    let credential = &config.relay.static_credentials[0];
    original
        .send(Message::Text(
            serde_json::to_string(&ClientControl::Hello {
                endpoint: credential.endpoint.clone(),
                token: credential.token.clone(),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    let Message::Text(text) = original.next().await.unwrap().unwrap() else {
        panic!("expected welcome");
    };
    assert!(matches!(
        serde_json::from_str::<ServerControl>(&text).unwrap(),
        ServerControl::Welcome { .. }
    ));

    for (request, rejection) in [
        (
            1,
            "Enabling database-backed services requires a process restart",
        ),
        (2, "exceeds semaphore capacity"),
    ] {
        let mut candidate = config.clone();
        if request == 1 {
            candidate.control.admin_ui.enabled = true;
            candidate.control.admin_ui.password_hash = "$argon2id$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E".into();
        } else {
            candidate.relay.limits.max_connections = tokio::sync::Semaphore::MAX_PERMITS + 1;
        }
        std::fs::write(&path, candidate.to_toml().unwrap()).unwrap();
        assert!(
            Command::new("kill")
                .args(["-HUP", &process.0.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "{}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            if std::fs::read_to_string(&log_path)
                .unwrap()
                .contains(rejection)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "missing reload rejection: {rejection}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let mut fresh = connect().await;
        fresh.close(None).await.unwrap();
        original
            .send(Message::Text(
                serde_json::to_string(&ClientControl::Connect {
                    request,
                    peer: "absent-peer".into(),
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
        let Message::Text(text) = original.next().await.unwrap().unwrap() else {
            panic!("the original relay session must remain usable");
        };
        assert!(matches!(
            serde_json::from_str::<ServerControl>(&text).unwrap(),
            ServerControl::Error { .. }
        ));
    }
    assert!(
        !config.sqlite_path.exists(),
        "rejected reload must not create storage"
    );
}
