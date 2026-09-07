//! Rate limiting middleware for AIS service
//!
//! Rate limiting policy:
//! - **Per IP**: at most 100 req/min per IP (burst of 100 requests)
//!
//! Implemented with tower-governor v0.8 to prevent DoS and resource exhaustion.

use axum::body::Body;
use governor::middleware::NoOpMiddleware;
use std::sync::Arc;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::PeerIpKeyExtractor,
};

/// Per-IP rate limiting configuration.
///
/// Limiting policy:
/// - 2 requests per second (120 req/min)
/// - burst of up to 100 requests
/// - keyed by client IP address
///
/// Uses the tower_governor v0.8.0 API.
///
/// Keying is based on the real transport peer IP (`PeerIpKeyExtractor`) rather
/// than `SmartIpKeyExtractor`. Without a trusted reverse-proxy configuration,
/// `SmartIpKeyExtractor` trusts `X-Forwarded-For` / `X-Real-IP` request headers,
/// which a client can forge to bypass rate limiting or to impersonate a victim
/// IP for a targeted DoS. The peer IP cannot be spoofed at this layer.
pub fn ip_rate_limiter() -> GovernorLayer<PeerIpKeyExtractor, NoOpMiddleware, Body> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2) // 2 requests per second
            .burst_size(100) // allow a burst of 100 requests
            .key_extractor(PeerIpKeyExtractor)
            .finish()
            .unwrap(),
    );

    GovernorLayer::new(governor_conf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_rate_limiter_creation() {
        let _limiter = ip_rate_limiter();
        // Successful creation means the configuration is valid.
    }
}
