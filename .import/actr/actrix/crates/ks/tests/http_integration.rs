use base64::Engine as _;
use ks::{
    GenerateKeyRequest, GenerateKeyResponse, GetSecretKeyResponse, KsServiceConfig,
    create_ks_state, create_router,
};
use nonce_auth::{CredentialBuilder, NonceCredential, storage::MemoryStorage};
use reqwest::StatusCode;
use serde_json::Value;
use tempfile::TempDir;
use tokio::net::TcpListener;

struct TestServer {
    base_url: String,
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
    let config = KsServiceConfig::default();
    let state = create_ks_state(&config, MemoryStorage::new(), psk, temp_dir.path())
        .await
        .expect("Failed to create KS state");
    let app = create_router(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().expect("Failed to read bound addr");
    let base_url = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("KS test server exited unexpectedly");
    });

    TestServer {
        base_url,
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
    let psk = "test-ks-psk";
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

    let credential = sign_request(psk, "generate_key");
    let generate_req = GenerateKeyRequest { credential };
    let generated = client
        .post(format!("{}/generate", server.base_url))
        .json(&generate_req)
        .send()
        .await
        .expect("generate request failed");
    assert_eq!(generated.status(), StatusCode::OK);

    let generated: GenerateKeyResponse = generated
        .json()
        .await
        .expect("generate response should parse");
    assert!(generated.key_id > 0);
    assert!(generated.expires_at > 0);

    let pub_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(generated.public_key.as_bytes())
        .expect("public key should be valid base64");
    assert_eq!(
        pub_key_bytes.len(),
        33,
        "Public key must be 33-byte compressed secp256k1 key"
    );

    let secret_payload = format!("get_secret_key:{}", generated.key_id);
    let secret_cred = sign_request(psk, &secret_payload);
    let query_params = [
        ("key_id", generated.key_id.to_string()),
        (
            "credential",
            serde_json::to_string(&secret_cred).expect("credential should serialize"),
        ),
    ];

    let secret_resp = client
        .get(format!("{}/secret/{}", server.base_url, generated.key_id))
        .query(&query_params)
        .send()
        .await
        .expect("secret request failed");
    assert_eq!(secret_resp.status(), StatusCode::OK);
    let secret: GetSecretKeyResponse = secret_resp
        .json()
        .await
        .expect("secret response should parse");
    assert_eq!(secret.key_id, generated.key_id);

    let sec_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(secret.secret_key.as_bytes())
        .expect("secret key should be valid base64");
    assert_eq!(sec_key_bytes.len(), 32, "Secret key must be 32 bytes");
}

#[tokio::test]
async fn test_http_generate_rejects_invalid_signature() {
    let psk = "test-ks-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let invalid_credential = sign_request(psk, "not-generate-key");
    let req = GenerateKeyRequest {
        credential: invalid_credential,
    };
    let response = client
        .post(format!("{}/generate", server.base_url))
        .json(&req)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_http_generate_replay_is_rejected() {
    let psk = "test-ks-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    // Build one credential and reuse twice to trigger nonce replay rejection.
    let credential = sign_request(psk, "generate_key");
    let req_body = GenerateKeyRequest {
        credential: credential.clone(),
    };

    let url = format!("{}/generate", server.base_url);
    let first = client.post(&url).json(&req_body).send().await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = client.post(&url).json(&req_body).send().await.unwrap();
    // replay should be rejected
    assert_eq!(second.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_http_generate_timestamp_out_of_window() {
    let psk = "test-ks-psk";
    let server = start_test_server(psk).await;
    let client = reqwest::Client::new();

    let mut credential = sign_request(psk, "generate_key");
    // push timestamp far into the past to exceed default window
    credential.timestamp = 0;
    let req_body = GenerateKeyRequest { credential };

    let url = format!("{}/generate", server.base_url);
    let resp = client.post(&url).json(&req_body).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
