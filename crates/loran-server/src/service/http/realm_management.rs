//! `POST /actr/realm-management`: Realm management requests
//! (`actr/protocols/realm-profile.md` section 8).
//!
//! The Realm domain's authority descriptor is fetched before any signature
//! is checked, so requests are limited per source address, and fetches per
//! target domain through a cache that also keeps failures.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::header,
    response::{IntoResponse, Response},
    routing::post,
};
use issuer::realm_management::{ManagementError, handle};
use issuer::realm_profile::RealmProfiles;
use platform::config::RealmConfig;
use platform::net::CachedFetcher;
use serde_json::json;
use std::sync::Arc;

struct ManagementState {
    profiles: Arc<RealmProfiles>,
    config: RealmConfig,
    fetcher: CachedFetcher,
}

async fn post_request(State(state): State<Arc<ManagementState>>, body: Bytes) -> Response {
    let fetcher = &state.fetcher;
    let result = handle(
        &state.profiles,
        &state.config,
        &body,
        |url: String| async move { fetcher.get(&url).await },
    )
    .await;
    // Replies can carry a Realm admin token.
    let no_store = [(header::CACHE_CONTROL, "no-store")];
    match result {
        Ok(reply) => (no_store, Json(reply)).into_response(),
        Err(error) => {
            if let ManagementError::Internal(message) = &error {
                platform::recording::warn!("Realm management request failed: {message}");
            }
            (
                error.status(),
                no_store,
                Json(json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    }
}

pub fn router(profiles: Arc<RealmProfiles>, config: RealmConfig) -> Router {
    let state = Arc::new(ManagementState {
        profiles,
        config,
        fetcher: CachedFetcher::default(),
    });
    Router::new()
        .route("/actr/realm-management", post(post_request))
        // Room for a Realm Profile of up to 64 KiB and the envelope.
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(issuer::ratelimit::ip_rate_limiter())
        .with_state(state)
}
