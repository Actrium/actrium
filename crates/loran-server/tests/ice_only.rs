//! Exercise UDP-only deployment through the regular service launcher.
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use loran_server::service::{IceService, TurnService};
use sha1::Sha1;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use stun_protocol::agent::TransactionId;
use stun_protocol::attributes::*;
use stun_protocol::integrity::MessageIntegrity;
use stun_protocol::message::*;
use tokio::net::UdpSocket;

struct Process {
    child: Child,
    log: PathBuf,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Process {
    fn assert_running(&mut self) {
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "server exited: {}",
            self.logs()
        );
    }

    fn logs(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    #[cfg(unix)]
    fn signal(&self, signal: &str) {
        assert!(
            Command::new("kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
    }
}

fn choose_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn write_config(directory: &Path, enable: u8, port: u16) -> PathBuf {
    let mut config = loran_server::LoranConfig::from_toml(include_str!(
        "../../../loran/config/ice.example.toml"
    ))
    .unwrap();
    config.enable = enable;
    config.env = "prod".into();
    config.bind.ice.ip = "127.0.0.1".into();
    config.bind.ice.port = port;
    config.bind.ice.advertised_ip = "127.0.0.1".into();
    config.turn.realm = "turn.example.org".into();
    config.turn.relay_port_range = "55000-55999".into();
    config.turn.users = [("alice".into(), "test-password".into())].into();
    // A real database initialization would fail against this regular file.
    config.sqlite_path = directory.join("storage-must-remain-a-file");
    config.pid = Some(directory.join("loran.pid").to_str().unwrap().into());
    let path = directory.join("ice.toml");
    std::fs::write(&path, config.to_toml().unwrap()).unwrap();
    path
}

fn request(method: Method) -> Message {
    let mut message = Message::new();
    message
        .build(&[
            Box::new(TransactionId::new()),
            Box::new(MessageType::new(method, CLASS_REQUEST)),
        ])
        .unwrap();
    message
}

async fn exchange(socket: &UdpSocket, message: &Message) -> Message {
    socket.send(&message.raw).await.unwrap();
    let mut buffer = [0; 2048];
    let len = tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let mut response = Message::new();
    response.write(&buffer[..len]).unwrap();
    assert_eq!(response.transaction_id, message.transaction_id);
    response
}

async fn wait_for_binding(process: &mut Process, port: u16) -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        process.assert_running();
        let binding = request(METHOD_BINDING);
        socket.send(&binding.raw).await.unwrap();
        let mut buffer = [0; 1024];
        if let Ok(Ok(len)) =
            tokio::time::timeout(Duration::from_millis(100), socket.recv(&mut buffer)).await
        {
            let mut response = Message::new();
            response.write(&buffer[..len]).unwrap();
            assert_eq!(response.typ, BINDING_SUCCESS);
            assert_eq!(response.transaction_id, binding.transaction_id);
            return socket;
        }
        assert!(
            Instant::now() < deadline,
            "UDP startup deadline: {}",
            process.logs()
        );
    }
}

async fn allocate_with(socket: &UdpSocket, username: &str, password: &str) -> Vec<u8> {
    let mut message = request(METHOD_ALLOCATE);
    message.add(ATTR_REQUESTED_TRANSPORT, &[17, 0, 0, 0]);
    let challenge = exchange(socket, &message).await;
    assert_eq!(challenge.typ.class, CLASS_ERROR_RESPONSE);
    assert_eq!(challenge.get(ATTR_REALM).unwrap(), b"turn.example.org");
    let nonce = challenge.get(ATTR_NONCE).unwrap();
    let mut message = request(METHOD_ALLOCATE);
    message.add(ATTR_REQUESTED_TRANSPORT, &[17, 0, 0, 0]);
    message.add(ATTR_USERNAME, username.as_bytes());
    message.add(ATTR_REALM, b"turn.example.org");
    message.add(ATTR_NONCE, &nonce);
    let key = turn_crate::auth::generate_auth_key(username, "turn.example.org", password);
    MessageIntegrity(key.clone()).add_to(&mut message).unwrap();
    let mut response = exchange(socket, &message).await;
    assert_eq!(response.typ.class, CLASS_SUCCESS_RESPONSE);
    MessageIntegrity(key).check(&mut response).unwrap();
    let address = response.get(ATTR_XOR_RELAYED_ADDRESS).unwrap();
    let port = u16::from_be_bytes([address[2], address[3]]) ^ 0x2112;
    assert!((55000..=55999).contains(&port));
    nonce.to_vec()
}

#[cfg(unix)]
async fn allocate_response(socket: &UdpSocket, username: &str, password: &str) -> Message {
    let mut message = request(METHOD_ALLOCATE);
    message.add(ATTR_REQUESTED_TRANSPORT, &[17, 0, 0, 0]);
    let challenge = exchange(socket, &message).await;
    assert_eq!(challenge.typ.class, CLASS_ERROR_RESPONSE);
    let nonce = challenge.get(ATTR_NONCE).unwrap();
    let mut message = request(METHOD_ALLOCATE);
    message.add(ATTR_REQUESTED_TRANSPORT, &[17, 0, 0, 0]);
    message.add(ATTR_USERNAME, username.as_bytes());
    message.add(ATTR_REALM, b"turn.example.org");
    message.add(ATTR_NONCE, &nonce);
    let key = turn_crate::auth::generate_auth_key(username, "turn.example.org", password);
    MessageIntegrity(key).add_to(&mut message).unwrap();
    exchange(socket, &message).await
}

async fn allocate(socket: &UdpSocket) {
    allocate_with(socket, "alice", "test-password").await;
}

async fn start_turn_service(
    config: loran_server::LoranConfig,
) -> (
    UdpSocket,
    tokio::sync::broadcast::Sender<()>,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let port = config.bind.ice.port;
    let (shutdown, _) = tokio::sync::broadcast::channel(1);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let mut service = TurnService::new(config);
    let receiver = shutdown.subscribe();
    let task = tokio::spawn(async move { service.start(receiver, ready_tx).await });
    tokio::time::timeout(Duration::from_secs(5), ready_rx)
        .await
        .expect("TURN service startup timed out")
        .expect("TURN service failed to start");
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client
        .connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    (client, shutdown, task)
}

fn test_turn_config() -> loran_server::LoranConfig {
    let mut config = loran_server::LoranConfig::from_toml(include_str!(
        "../../../loran/config/ice.example.toml"
    ))
    .unwrap();
    config.env = "dev".into();
    config.enable = 4;
    config.bind.ice.ip = "127.0.0.1".into();
    config.bind.ice.port = choose_port();
    config.bind.ice.advertised_ip = "127.0.0.1".into();
    config.turn.realm = "turn.example.org".into();
    config.turn.relay_port_range = "55000-55999".into();
    config
}

#[tokio::test]
async fn standalone_shared_secret_accepts_external_rest_credentials() {
    let secret = "qQ3ry54gn6iDjz02rBGn3YRmxZv8bqsf1qB9Gcb2";
    let mut config = test_turn_config();
    config.turn.users.clear();
    config.turn.turn_secret = secret.into();
    assert!(config.validate().is_ok());
    let (client, shutdown, task) = start_turn_service(config).await;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let username = format!("{expires}:external-client");
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(username.as_bytes());
    let password = STANDARD.encode(mac.finalize().into_bytes());
    allocate_with(&client, &username, &password).await;
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn turn_auth_mode_reload_replaces_the_old_rest_authenticator() {
    let directory = tempfile::tempdir().unwrap();
    let secret = "qQ3ry54gn6iDjz02rBGn3YRmxZv8bqsf1qB9Gcb2";
    let mut config = test_turn_config();
    config.turn.users.clear();
    config.turn.turn_secret = secret.into();
    config.sqlite_path = directory.path().join("database");
    config.loran_shared_key = "b2xISzUdwmiURo7wKFL6K9tbkRHhTfM2E83r".into();
    let http = platform::config::bind::HttpBindConfig {
        ip: "127.0.0.1".into(),
        port: std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port(),
        ..Default::default()
    };
    config.bind.http = Some(http);
    assert!(config.validate().is_ok(), "{:?}", config.validate());
    assert!(!config.uses_realm_turn_auth());
    let path = directory.path().join("ice.toml");
    let log = directory.path().join("server.log");
    std::fs::write(&path, config.to_toml().unwrap()).unwrap();
    let output = std::fs::File::create(&log).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_loran"))
        .arg("--config")
        .arg(&path)
        .current_dir(directory.path())
        .stdout(output.try_clone().unwrap())
        .stderr(output)
        .spawn()
        .unwrap();
    let mut process = Process { child, log };
    let client = wait_for_binding(&mut process, config.bind.ice.port).await;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let username = format!("{expires}:external-client");
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(username.as_bytes());
    let password = STANDARD.encode(mac.finalize().into_bytes());
    allocate_with(&client, &username, &password).await;

    // Let the launcher install its SIGHUP handler before triggering reload.
    tokio::time::sleep(Duration::from_millis(100)).await;
    config.authority.uri = Some("actr://provider.example".into());
    assert!(config.uses_realm_turn_auth());
    std::fs::write(&path, config.to_toml().unwrap()).unwrap();
    process.signal("-HUP");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !process
        .logs()
        .contains("Configuration reload completed successfully")
    {
        process.assert_running();
        assert!(Instant::now() < deadline, "{}", process.logs());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let client = wait_for_binding(&mut process, config.bind.ice.port).await;
    let response = allocate_response(&client, &username, &password).await;
    assert_eq!(response.typ.class, CLASS_ERROR_RESPONSE);
    let code = response.get(ATTR_ERROR_CODE).unwrap();
    assert_eq!(((code[2] & 7) as u16) * 100 + code[3] as u16, 401);
}

#[tokio::test]
async fn static_turn_allocation_survives_an_unrelated_realm_suspension() {
    let directory = tempfile::tempdir().unwrap();
    platform::storage::db::set_db_path(directory.path())
        .await
        .unwrap();
    platform::realm::Realm::create("other@example.com".into())
        .await
        .unwrap();
    let mut config = test_turn_config();
    config.bind.http = Some(Default::default());
    config.turn.users = [("alice".into(), "test-password".into())].into();
    assert!(config.requires_database());
    let (client, shutdown, task) = start_turn_service(config).await;
    let nonce = allocate_with(&client, "alice", "test-password").await;

    platform::realm::Realm::set_enabled("other@example.com", false)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    let mut refresh = request(METHOD_REFRESH);
    refresh.add(ATTR_USERNAME, b"alice");
    refresh.add(ATTR_REALM, b"turn.example.org");
    refresh.add(ATTR_NONCE, &nonce);
    MessageIntegrity(turn_crate::auth::generate_auth_key(
        "alice",
        "turn.example.org",
        "test-password",
    ))
    .add_to(&mut refresh)
    .unwrap();
    assert_eq!(
        exchange(&client, &refresh).await.typ.class,
        CLASS_SUCCESS_RESPONSE
    );
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn regular_launcher_runs_udp_only_without_http_or_database() {
    for enable in [2, 4, 6] {
        let directory = tempfile::tempdir().unwrap();
        let storage = directory.path().join("storage-must-remain-a-file");
        std::fs::write(&storage, b"untouched").unwrap();
        let port = choose_port();
        let config = write_config(directory.path(), enable, port);
        let validation = Command::new(env!("CARGO_BIN_EXE_loran"))
            .arg("test")
            .arg(&config)
            .output()
            .unwrap();
        assert!(
            validation.status.success(),
            "{}",
            String::from_utf8_lossy(&validation.stderr)
        );
        let log = directory.path().join("server.log");
        let output = std::fs::File::create(&log).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_loran"))
            .arg("--config")
            .arg(&config)
            .current_dir(directory.path())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap();
        let mut process = Process { child, log };
        let client = wait_for_binding(&mut process, port).await;
        if enable & 4 != 0 {
            allocate(&client).await;
        }

        #[cfg(unix)]
        {
            // Wait until startup has reached the SIGHUP handler before reloading.
            tokio::time::sleep(Duration::from_millis(100)).await;
            let next_port = choose_port();
            write_config(directory.path(), enable, next_port);
            process.signal("-HUP");
            let reloaded = wait_for_binding(&mut process, next_port).await;
            if enable & 4 != 0 {
                allocate(&reloaded).await;
            }
            process.signal("-TERM");
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = process.child.try_wait().unwrap() {
                    assert!(status.success(), "{}", process.logs());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "shutdown deadline: {}",
                    process.logs()
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(!directory.path().join("loran.pid").exists());
        }
        assert_eq!(std::fs::read(&storage).unwrap(), b"untouched");
        assert!(!directory.path().join("database").exists());
        assert!(!directory.path().join("loran.db").exists());
        let logs = process.logs();
        assert!(!logs.contains("Database initialization complete"), "{logs}");
        assert!(!logs.contains("Starting HTTP server"), "{logs}");
        assert!(!logs.contains("Starting HTTPS server"), "{logs}");
    }
}
