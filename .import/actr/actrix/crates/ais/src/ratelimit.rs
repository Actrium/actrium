//! Rate limiting middleware for AIS service
//!
//! Rate limiting policy:
//! - **IP level**: max 100 req/min per IP (burst of 100 requests)
//!
//! Uses tower-governor v0.8 to implement rate limiting, preventing DoS attacks and resource exhaustion

use axum::body::Body;
use governor::middleware::NoOpMiddleware;
use std::sync::Arc;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};

/// IP-level rate limiter configuration
///
/// Limiting policy:
/// - 2 requests per second (120 req/min)
/// - Burst of 100 requests allowed
/// - Rate limited by client IP address
///
/// Uses tower_governor v0.8.0 API
pub fn ip_rate_limiter() -> GovernorLayer<SmartIpKeyExtractor, NoOpMiddleware, Body> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2) // 2 requests per second
            .burst_size(100) // allow a burst of 100 requests
            .key_extractor(SmartIpKeyExtractor)
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
        // If creation succeeds, the configuration is valid
    }
}
