//! Exercise WSS in fresh processes so another test cannot initialize Rustls first.

use actr_hyper::wire::webrtc::{
    InboundQueueConfig, ReconnectConfig, SignalingClient, SignalingConfig, WebSocketSignalingClient,
};
use futures_util::StreamExt;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{path::Path, process::Command, sync::Arc, time::Duration};
use tokio_rustls::{TlsAcceptor, rustls};

#[test]
fn wss_selects_a_provider_and_still_verifies_certificates() {
    const CASE: &str = "ACTR_TEST_WSS_TLS_CASE";
    if let Ok(case) = std::env::var(CASE) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(check_wss(&case));
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let tls = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    std::fs::write(directory.path().join("server.pem"), tls.cert.pem()).unwrap();
    std::fs::write(
        directory.path().join("server-key.pem"),
        tls.signing_key.serialize_pem(),
    )
    .unwrap();
    let unrelated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    std::fs::write(directory.path().join("untrusted.pem"), unrelated.cert.pem()).unwrap();

    for (case, root) in [("trusted", "server.pem"), ("untrusted", "untrusted.pem")] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "wss_selects_a_provider_and_still_verifies_certificates",
                "--nocapture",
            ])
            .env(CASE, case)
            .env("ACTR_TEST_WSS_TLS_DIRECTORY", directory.path())
            .env("SSL_CERT_FILE", directory.path().join(root))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

async fn check_wss(case: &str) {
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    let directory = std::env::var("ACTR_TEST_WSS_TLS_DIRECTORY").unwrap();
    let directory = Path::new(&directory);
    // The server chooses its own provider without installing a process default.
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from_pem_file(directory.join("server.pem")).unwrap()],
        PrivateKeyDer::from_pem_file(directory.join("server-key.pem")).unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        if let Ok(tls) = TlsAcceptor::from(Arc::new(config)).accept(socket).await {
            let mut websocket = tokio_tungstenite::accept_async(tls).await.unwrap();
            while let Some(Ok(message)) = websocket.next().await {
                if message.is_close() {
                    let _ = websocket.close(None).await;
                    break;
                }
            }
        }
    });
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    let client = Arc::new(WebSocketSignalingClient::new(SignalingConfig {
        server_url: format!("wss://{address}/signaling/ws").parse().unwrap(),
        connection_timeout: 5,
        heartbeat_interval: 30,
        reconnect_config: ReconnectConfig {
            enabled: false,
            ..ReconnectConfig::default()
        },
        auth_config: None,
        webrtc_role: None,
        inbound_queue: InboundQueueConfig::default(),
    }));
    let result = client.connect().await;
    if case == "trusted" {
        result.expect("connect with the test CA and no preinstalled CryptoProvider");
        client.disconnect().await.unwrap();
    } else {
        let error = result.expect_err("an untrusted certificate must still be refused");
        assert!(error.to_string().contains("certificate"), "{error}");
    }
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
