//! Publisher document transport for the disposable package-runtime E2E server.
//! Compiled only into test binaries. Production always uses public HTTPS.

use anyhow::Result;
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use issuer::handlers::IssuerState;
use issuer::publisher_profiles::{PublicDocumentFetcher, PublisherProfileRefresher};
use platform::net::FetchError;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const DOMAIN: &str = "e2e.actrium.example";

pub(super) fn configure(mut state: IssuerState) -> Result<IssuerState> {
    if std::env::var("ACTR_PACKAGE_RUNTIME_FIXTURE").as_deref() != Ok("1") {
        return Ok(state);
    }
    let issuer = Arc::try_unwrap(state.issuer).map_err(|_| {
        anyhow::anyhow!("fixture Issuer was shared before document transport setup")
    })?;
    let fetcher = Arc::new(PublisherDocument {
        root: SigningKey::from_bytes(&rand::random()),
        sequence: AtomicU64::new(1),
    });
    state.issuer = Arc::new(
        issuer.with_publisher_profiles(Arc::new(PublisherProfileRefresher::with_defaults(fetcher))),
    );
    Ok(state)
}

struct PublisherDocument {
    root: SigningKey,
    sequence: AtomicU64,
}

#[async_trait::async_trait]
impl PublicDocumentFetcher for PublisherDocument {
    async fn fetch(&self, url: &str) -> Result<Arc<[u8]>, FetchError> {
        if url != format!("https://{DOMAIN}/.well-known/actr") {
            return Err(FetchError::new(
                "unexpected publisher domain in E2E fixture",
            ));
        }
        if std::env::var_os("ACTR_REALM_E2E_HTTPS_CONFIG").is_some() {
            return https_documents::fetch(url.to_string())
                .await
                .map(Arc::from)
                .map_err(|error| FetchError::new(error.to_string()));
        }
        // The shell provisions public package keys into its disposable DB.
        // Represent them in a signed authority descriptor; signature, pin,
        // sequence and revocation acceptance still use production code.
        let pool = platform::storage::db::get_database().get_pool();
        let package: (i64, String, String) = sqlx::query_as(
            "SELECT id, key_id, public_key FROM publisher WHERE name = ? AND status = 'active'",
        )
        .bind(DOMAIN)
        .fetch_one(pool)
        .await
        .map_err(|e| FetchError::new(e.to_string()))?;
        let member = |id: &str, public_key: &str, status: &str| json!({"id": id, "algorithm": "Ed25519", "public_key": public_key, "status": status});
        let mut keys = vec![member(&package.1, &package.2, "active")];
        let retired: Vec<(String, String)> = sqlx::query_as(
            "SELECT key_id, public_key FROM publisher_key_history WHERE publisher_id = ? AND status = 'retired'",
        )
        .bind(package.0)
        .fetch_all(pool)
        .await
        .map_err(|e| FetchError::new(e.to_string()))?;
        keys.extend(retired.iter().map(|(id, key)| member(id, key, "retired")));
        let now = chrono::Utc::now();
        let root = base64::prelude::BASE64_STANDARD.encode(self.root.verifying_key().as_bytes());
        let mut profile = json!({
            "schema": "actr.publisher/v1",
            "sequence": self.sequence.fetch_add(1, Ordering::SeqCst),
            "domain": DOMAIN,
            "profile_signing_keys": [member("fixture-root", &root, "active")],
            "keys": keys,
            "profile_issued_at": (now - chrono::Duration::minutes(1)).to_rfc3339(),
            "profile_expires_at": (now + chrono::Duration::days(1)).to_rfc3339(),
        });
        actr_protocol::realm_profile::sign_document(&mut profile, "fixture-root", &self.root);
        serde_json::to_vec(&json!({
            "version": 1,
            "authority": format!("actr://{DOMAIN}"),
            "services": [{"rel": "contracts", "href": format!("https://{DOMAIN}/contracts")}],
            "publisher": profile,
        }))
        .map(Arc::from)
        .map_err(|e| FetchError::new(e.to_string()))
    }
}

mod https_documents {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/e2e/actr/package-runtime-echo/fixtures/documents.rs"
    ));
}
