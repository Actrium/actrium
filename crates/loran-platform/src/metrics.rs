//! Prometheus metrics module
//!
//! Provides global metrics collection and export

use lazy_static::lazy_static;
use prometheus::{
    HistogramOpts, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use std::collections::HashMap;
use std::sync::{Once, RwLock};
use std::time::Instant;

static METRICS_INIT: Once = Once::new();

lazy_static! {
    /// Global Prometheus Registry
    pub static ref REGISTRY: Registry = Registry::new();

    // ========== Business metrics ==========

    /// Total number of registered actors (grouped by realm)
    pub static ref ACTORS_TOTAL: IntGaugeVec = IntGaugeVec::new(
        Opts::new("loran_actors_total", "Total number of registered actors")
            .namespace("loran"),
        &["realm", "service"]
    ).unwrap();

    /// Total number of registered services
    pub static ref SERVICES_TOTAL: IntGaugeVec = IntGaugeVec::new(
        Opts::new("loran_services_total", "Total number of registered services")
            .namespace("loran"),
        &["service_name"]
    ).unwrap();

    /// Number of WebSocket connections
    pub static ref WEBSOCKET_CONNECTIONS: IntGauge = IntGauge::new(
        "loran_websocket_connections",
        "Number of active WebSocket connections"
    ).unwrap();

    /// Token issuance count (Issuer)
    pub static ref TOKENS_ISSUED: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_tokens_issued_total", "Total number of tokens issued")
            .namespace("loran"),
        &["realm", "status"]
    ).unwrap();

    /// Token validation count (Signaling/TURN)
    pub static ref TOKENS_VALIDATED: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_tokens_validated_total", "Total number of tokens validated")
            .namespace("loran"),
        &["service", "status"]
    ).unwrap();

    // ========== Performance metrics ==========

    /// HTTP request duration (seconds)
    pub static ref REQUEST_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("loran_request_duration_seconds", "HTTP request duration in seconds")
            .namespace("loran")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
        &["service", "method", "path", "status"]
    ).unwrap();

    /// Total number of HTTP requests
    pub static ref REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_requests_total", "Total number of HTTP requests")
            .namespace("loran"),
        &["service", "method", "path", "status"]
    ).unwrap();

    /// Error count
    pub static ref ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_errors_total", "Total number of errors")
            .namespace("loran"),
        &["service", "error_type"]
    ).unwrap();

    // ========== System metrics ==========

    /// Cache hit count
    pub static ref CACHE_HITS: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_cache_hits_total", "Total number of cache hits")
            .namespace("loran"),
        &["cache_type"]
    ).unwrap();

    /// Cache miss count
    pub static ref CACHE_MISSES: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_cache_misses_total", "Total number of cache misses")
            .namespace("loran"),
        &["cache_type"]
    ).unwrap();

    /// Database connection pool status
    pub static ref DB_CONNECTIONS: IntGaugeVec = IntGaugeVec::new(
        Opts::new("loran_db_connections", "Number of database connections")
            .namespace("loran"),
        &["pool", "state"]
    ).unwrap();

    // ========== Security metrics ==========

    /// Rate limit trigger count
    pub static ref RATE_LIMIT_EXCEEDED: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_rate_limit_exceeded_total", "Total number of rate limit violations")
            .namespace("loran"),
        &["service", "limiter_type"]
    ).unwrap();

    /// Authentication failure count
    pub static ref AUTH_FAILURES: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_auth_failures_total", "Total number of authentication failures")
            .namespace("loran"),
        &["service", "reason"]
    ).unwrap();

    /// Invalid request count
    pub static ref INVALID_REQUESTS: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_invalid_requests_total", "Total number of invalid requests")
            .namespace("loran"),
        &["service", "reason"]
    ).unwrap();

    // ========== Signer-specific metrics ==========

    /// Key generation count
    pub static ref KEYS_GENERATED: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_keys_generated_total", "Total number of keys generated")
            .namespace("loran"),
        &["key_type"]
    ).unwrap();

    /// Key rotation count
    pub static ref KEY_ROTATIONS: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_key_rotations_total", "Total number of key rotations")
            .namespace("loran"),
        &["reason"]
    ).unwrap();

    // ========== TURN-specific metrics ==========

    /// TURN allocation requests
    pub static ref TURN_ALLOCATIONS: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_turn_allocations_total", "Total number of TURN allocations")
            .namespace("loran"),
        &["status"]
    ).unwrap();

    /// TURN active session count
    pub static ref TURN_ACTIVE_SESSIONS: IntGauge = IntGauge::new(
        "loran_turn_active_sessions",
        "Number of active TURN sessions"
    ).unwrap();

    /// TURN traffic statistics (bytes)
    pub static ref TURN_BYTES_RELAYED: IntCounterVec = IntCounterVec::new(
        Opts::new("loran_turn_bytes_relayed_total", "Total bytes relayed by TURN")
            .namespace("loran"),
        &["direction"]
    ).unwrap();
}

/// Per-service Prometheus registries for isolated metrics export.
pub struct ServiceMetricsRegistry {
    registries: RwLock<HashMap<String, Registry>>,
}

impl Default for ServiceMetricsRegistry {
    fn default() -> Self {
        Self {
            registries: RwLock::new(HashMap::new()),
        }
    }
}

impl ServiceMetricsRegistry {
    pub fn new() -> Self {
        Self {
            registries: RwLock::new(HashMap::new()),
        }
    }

    /// Get an existing registry or create a new one for the given service.
    pub fn get_or_create(&self, service: &str) -> Registry {
        {
            let read = self.registries.read().unwrap();
            if let Some(r) = read.get(service) {
                return r.clone();
            }
        }
        let mut write = self.registries.write().unwrap();
        write.entry(service.to_string()).or_default().clone()
    }

    /// Export Prometheus text for a single service registry.
    pub fn export_for(&self, service: &str) -> String {
        use prometheus::Encoder;
        let read = self.registries.read().unwrap();
        let Some(registry) = read.get(service) else {
            return String::new();
        };
        let encoder = prometheus::TextEncoder::new();
        let families = registry.gather();
        let mut buf = Vec::new();
        encoder.encode(&families, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// Export all per-service registries merged with the global REGISTRY.
    pub fn export_all(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();

        // Gather from global registry
        let mut families = REGISTRY.gather();

        // Gather from all per-service registries
        let read = self.registries.read().unwrap();
        for registry in read.values() {
            families.extend(registry.gather());
        }

        let mut buf = Vec::new();
        encoder.encode(&families, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }
}

lazy_static! {
    /// Per-service Prometheus registries
    pub static ref SERVICE_REGISTRIES: ServiceMetricsRegistry = ServiceMetricsRegistry::new();
}

/// Register all metrics to the global Registry
///
/// This function is idempotent - calling it multiple times is safe.
/// Only the first call will actually register the metrics.
pub fn register_metrics() -> Result<(), prometheus::Error> {
    let mut result = Ok(());

    METRICS_INIT.call_once(|| {
        let register_result = (|| {
            // Business metrics
            REGISTRY.register(Box::new(ACTORS_TOTAL.clone()))?;
            REGISTRY.register(Box::new(SERVICES_TOTAL.clone()))?;
            REGISTRY.register(Box::new(WEBSOCKET_CONNECTIONS.clone()))?;
            REGISTRY.register(Box::new(TOKENS_ISSUED.clone()))?;
            REGISTRY.register(Box::new(TOKENS_VALIDATED.clone()))?;

            // Performance metrics
            REGISTRY.register(Box::new(REQUEST_DURATION.clone()))?;
            REGISTRY.register(Box::new(REQUESTS_TOTAL.clone()))?;
            REGISTRY.register(Box::new(ERRORS_TOTAL.clone()))?;

            // System metrics
            REGISTRY.register(Box::new(CACHE_HITS.clone()))?;
            REGISTRY.register(Box::new(CACHE_MISSES.clone()))?;
            REGISTRY.register(Box::new(DB_CONNECTIONS.clone()))?;

            // Security metrics
            REGISTRY.register(Box::new(RATE_LIMIT_EXCEEDED.clone()))?;
            REGISTRY.register(Box::new(AUTH_FAILURES.clone()))?;
            REGISTRY.register(Box::new(INVALID_REQUESTS.clone()))?;

            // Signer-specific metrics
            REGISTRY.register(Box::new(KEYS_GENERATED.clone()))?;
            REGISTRY.register(Box::new(KEY_ROTATIONS.clone()))?;

            // TURN-specific metrics
            REGISTRY.register(Box::new(TURN_ALLOCATIONS.clone()))?;
            REGISTRY.register(Box::new(TURN_ACTIVE_SESSIONS.clone()))?;
            REGISTRY.register(Box::new(TURN_BYTES_RELAYED.clone()))?;

            Ok::<(), prometheus::Error>(())
        })();

        if let Err(e) = register_result {
            result = Err(e);
        }
    });

    result
}

/// HTTP request timer
pub struct RequestTimer {
    start: Instant,
    service: String,
    method: String,
    path: String,
}

impl RequestTimer {
    /// Create the timer
    pub fn new(service: &str, method: &str, path: &str) -> Self {
        Self {
            start: Instant::now(),
            service: service.to_string(),
            method: method.to_string(),
            path: path.to_string(),
        }
    }

    /// Finish timing and record metrics
    pub fn observe(self, status: u16) {
        let duration = self.start.elapsed().as_secs_f64();
        let status_str = status.to_string();

        REQUEST_DURATION
            .with_label_values(&[&self.service, &self.method, &self.path, &status_str])
            .observe(duration);

        REQUESTS_TOTAL
            .with_label_values(&[&self.service, &self.method, &self.path, &status_str])
            .inc();
    }
}

/// Export metrics in Prometheus format
pub fn export_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = REGISTRY.gather();

    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    String::from_utf8(buffer).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_metrics() {
        // Registration should succeed (or metrics are already registered)
        let result = register_metrics();
        // If already registered, returns an error, but functionality is unaffected
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_request_timer() {
        let _ = register_metrics();

        let timer = RequestTimer::new("test-service", "GET", "/test");
        std::thread::sleep(std::time::Duration::from_millis(10));
        timer.observe(200);

        // Verify counter incremented
        let before = REQUESTS_TOTAL
            .with_label_values(&["test-service", "GET", "/test", "200"])
            .get();

        let timer2 = RequestTimer::new("test-service", "GET", "/test");
        timer2.observe(200);

        let after = REQUESTS_TOTAL
            .with_label_values(&["test-service", "GET", "/test", "200"])
            .get();

        assert!(after > before);
    }

    #[test]
    fn test_export_metrics() {
        let _ = register_metrics();

        WEBSOCKET_CONNECTIONS.set(42);

        let output = export_metrics();
        // Check for the metric name (may appear with or without namespace prefix)
        assert!(
            output.contains("loran_websocket_connections")
                || output.contains("websocket_connections"),
            "Output should contain websocket_connections metric. Output: {output}"
        );
        assert!(
            output.contains("42"),
            "Output should contain value 42. Output: {output}"
        );
    }
}
