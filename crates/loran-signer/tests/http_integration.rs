use nonce_auth::{CredentialBuilder, NonceCredential, storage::MemoryStorage};
use reqwest::StatusCode;
use serde_json::Value;
use signer::{
    SignerServiceConfig, create_router, create_signer_state,
    types::{SignRequest, sign_request_payload},
};
use tempfile::TempDir;
use tokio::net::TcpListener;

struct TestServer {
    base_url: String,
    /// A named key created before the server started.
    key_id: u32,
    _temp_dir: TempDir,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn start_test_server(psk: &str) -> TestServer {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let config = SignerServiceConfig::default();
    let state = create_signer_state(&config, MemoryStorage::new(), psk, temp_dir.path())
        .await
        .expect("Failed to create Signer state");
    let key_id = state
        .storage
        .ensure_named_key("test")
        .await
        .expect("Failed to create a named key")
        .key_id;
    let app = create_router(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().expect("Failed to read bound addr");
    let base_url = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("Signer test server exited unexpectedly");
    });

    TestServer {
        base_url,
        key_id,
        _temp_dir: temp_dir,
        handle,
    }
}

fn sign_request(psk: &str, payload: &str) -> NonceCredential {
    CredentialBuilder::new(psk.as_bytes())
        .sign(payload.as_bytes())
        .expect("Failed to sign nonce credential")
}

#[tokio::test]
async fn test_http_health_and_key_lifecycle() {
    let psk = "test-signer-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let health = client
        .get(format!("{}/health", server.base_url))
        .send()
        .await
        .expect("health request failed");
    assert_eq!(health.status(), StatusCode::OK);
    let health_json: Value = health.json().await.expect("health body should be json");
    assert_eq!(health_json["status"], "healthy");

    let key_id = server.key_id;

    // test sign endpoint
    let message = b"hello, world";
    let sign_payload = sign_request_payload(key_id, message);
    let sign_cred = sign_request(psk, &sign_payload);
    let sign_req = SignRequest {
        key_id,
        message: message.to_vec(),
        credential: sign_cred,
    };
    let sign_resp = client
        .post(format!("{}/sign/{}", server.base_url, key_id))
        .json(&sign_req)
        .send()
        .await
        .expect("sign request failed");
    assert_eq!(sign_resp.status(), StatusCode::OK);
    let sign_body: Value = sign_resp.json().await.expect("sign response should parse");
    // signature field should be a byte array (JSON array of numbers or base64)
    assert!(sign_body["signature"].is_array() || sign_body["signature"].is_string());
}

fn sign_body(psk: &str, key_id: u32) -> SignRequest {
    SignRequest {
        key_id,
        message: b"test".to_vec(),
        credential: sign_request(psk, &sign_request_payload(key_id, b"test")),
    }
}

#[tokio::test]
async fn test_http_sign_replay_is_rejected() {
    let psk = "test-signer-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let req_body = sign_body(psk, server.key_id);
    let url = format!("{}/sign/{}", server.base_url, server.key_id);
    let first = client.post(&url).json(&req_body).send().await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = client.post(&url).json(&req_body).send().await.unwrap();
    // replay should be rejected
    assert_eq!(second.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_http_sign_timestamp_out_of_window() {
    let psk = "test-signer-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let mut req_body = sign_body(psk, server.key_id);
    // push timestamp far into the past to exceed default window
    req_body.credential.timestamp = 0;

    let url = format!("{}/sign/{}", server.base_url, server.key_id);
    let resp = client.post(&url).json(&req_body).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_http_sign_rejects_invalid_signature() {
    let psk = "test-signer-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let key_id = server.key_id;

    // use wrong payload for credential
    let invalid_cred = sign_request(psk, "wrong-payload");
    let sign_req = SignRequest {
        key_id,
        message: b"test".to_vec(),
        credential: invalid_cred,
    };

    let resp = client
        .post(format!("{}/sign/{}", server.base_url, key_id))
        .json(&sign_req)
        .send()
        .await
        .expect("sign request should complete");

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_http_sign_authentication_binds_exact_message_and_path_key() {
    let psk = "test-ks-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();
    let key_id = server.key_id;
    let approved_message = b"approved";
    let credential = sign_request(psk, &sign_request_payload(key_id, approved_message));

    let tampered = SignRequest {
        key_id,
        message: b"tampered".to_vec(),
        credential,
    };
    let response = client
        .post(format!("{}/sign/{}", server.base_url, key_id))
        .json(&tampered)
        .send()
        .await
        .expect("tampered request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let wrong_path_message = b"approved";
    let wrong_path = SignRequest {
        key_id,
        message: wrong_path_message.to_vec(),
        credential: sign_request(psk, &sign_request_payload(key_id, wrong_path_message)),
    };
    let response = client
        .post(format!("{}/sign/{}", server.base_url, key_id + 1))
        .json(&wrong_path)
        .send()
        .await
        .expect("path mismatch request should complete");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_http_sign_nonexistent_key_returns_not_found() {
    let psk = "test-signer-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let missing_key_id = 99_999_u32;
    let message = b"test";
    let sign_payload = sign_request_payload(missing_key_id, message);
    let sign_cred = sign_request(psk, &sign_payload);
    let sign_req = SignRequest {
        key_id: missing_key_id,
        message: message.to_vec(),
        credential: sign_cred,
    };

    let resp = client
        .post(format!("{}/sign/{missing_key_id}", server.base_url))
        .json(&sign_req)
        .send()
        .await
        .expect("sign request should complete");

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
