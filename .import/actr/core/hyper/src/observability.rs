//! Observability module for logging and tracing initialization.
//!
//! This module provides unified initialization for logging (via `tracing`) and
//! optional distributed tracing (via OpenTelemetry). It supports injecting
//! custom platform-specific layers (e.g., Android Logcat, iOS os_log) while
//! providing a sensible default (stderr fmt layer) when none is provided.

use actr_config::ObservabilityConfig;
use actr_protocol::ActorResult;
#[cfg(feature = "opentelemetry")]
use opentelemetry::{KeyValue, trace::TracerProvider as _};
#[cfg(feature = "opentelemetry")]
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
#[cfg(feature = "opentelemetry")]
use opentelemetry_otlp::tonic_types::transport::{Certificate, ClientTlsConfig};
#[cfg(feature = "opentelemetry")]
use opentelemetry_otlp::{WithExportConfig, WithTonicConfig};
#[cfg(feature = "opentelemetry")]
use opentelemetry_sdk::{
    logs::SdkLoggerProvider, propagation::TraceContextPropagator, resource::Resource,
    trace::SdkTracerProvider,
};
#[cfg(feature = "opentelemetry")]
use tracing_subscriber::filter::Targets;
use tracing_subscriber::{
    Layer, filter::EnvFilter, fmt, layer::SubscriberExt, prelude::*, registry::LookupSpan,
};

/// Type alias for a boxed tracing layer that can be dynamically composed.
///
/// Platform-specific bindings (e.g., `libactr` for Swift/Kotlin) can create
/// layers using `tracing-android` or `tracing-oslog` and pass them here.
type BoxedLayer<S> = Box<dyn Layer<S> + Send + Sync + 'static>;

/// Guard for observability resources. Shuts down tracing exporter on drop.
#[derive(Default)]
pub struct ObservabilityGuard {
    #[cfg(feature = "opentelemetry")]
    tracer_provider: Option<SdkTracerProvider>,
    #[cfg(feature = "opentelemetry")]
    logger_provider: Option<SdkLoggerProvider>,
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        #[cfg(feature = "opentelemetry")]
        {
            if let Some(provider) = self.tracer_provider.take() {
                if let Err(err) = provider.shutdown() {
                    tracing::warn!("Failed to shutdown tracer provider: {err:?}");
                }
            }
            if let Some(provider) = self.logger_provider.take() {
                if let Err(err) = provider.shutdown() {
                    tracing::warn!("Failed to shutdown logger provider: {err:?}");
                }
            }
        }
    }
}

/// Initialize logging + (optional) tracing subscriber with default fmt layer.
///
/// This is the original API for backward compatibility. It uses a stderr-based
/// fmt layer for local logging output.
///
/// - `RUST_LOG` wins over configured level; fallback to `info` if unset.
/// - Tracing exporter only activates when both the `opentelemetry` feature is enabled and
///   `cfg.tracing_enabled` is true.
/// - Invalid endpoints fail fast; runtime delivery errors log but do not abort.
pub fn init_observability(
    cfg: &actr_config::ObservabilityConfig,
) -> ActorResult<ObservabilityGuard> {
    init_observability_with_layer(cfg, None::<BoxedLayer<tracing_subscriber::Registry>>)
}

/// Initialize logging + (optional) tracing subscriber with a custom platform layer.
///
/// This extended API allows platform-specific bindings to inject their own
/// logging layer (e.g., `tracing-android` for Logcat, `tracing-oslog` for Apple).
///
/// # Arguments
///
/// * `cfg` - Observability configuration (filter level, OTel settings)
/// * `platform_layer` - Optional custom layer for platform-specific logging.
///   If `None`, a default `fmt::layer()` outputting to stderr will be used.
///
/// # Example
///
/// ```rust,ignore
/// // In libactr for Android:
/// let android_layer = tracing_android::layer("actr")
///     .expect("Failed to create Android layer");
/// let guard = init_observability_with_layer(&cfg, Some(android_layer.boxed()))?;
/// ```
pub fn init_observability_with_layer<L>(
    cfg: &ObservabilityConfig,
    platform_layer: Option<L>,
) -> ActorResult<ObservabilityGuard>
where
    L: Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    let level_directive = std::env::var("RUST_LOG")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.filter_level.clone());
    let env_filter =
        EnvFilter::try_new(level_directive.clone()).unwrap_or_else(|_| EnvFilter::new("info"));

    init_subscriber_internal(cfg, env_filter, platform_layer)
}

// ============================================================================
// Internal implementation
// ============================================================================

#[cfg(not(feature = "opentelemetry"))]
fn init_subscriber_internal<L>(
    _cfg: &ObservabilityConfig,
    env_filter: EnvFilter,
    platform_layer: Option<L>,
) -> ActorResult<ObservabilityGuard>
where
    L: Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    // Apply the filter to the output layer using with_filter()
    // This ensures the filter properly gates events before they reach the output layer
    let filtered_layer = if let Some(layer) = platform_layer {
        layer.with_filter(env_filter).boxed()
    } else {
        create_default_fmt_layer().with_filter(env_filter).boxed()
    };

    let _ = tracing_subscriber::registry()
        .with(filtered_layer)
        .try_init();

    Ok(ObservabilityGuard::default())
}

#[cfg(feature = "opentelemetry")]
fn init_subscriber_internal<L>(
    cfg: &ObservabilityConfig,
    env_filter: EnvFilter,
    platform_layer: Option<L>,
) -> ActorResult<ObservabilityGuard>
where
    L: Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
    // Apply the filter to the output layer using with_filter()
    // This ensures the filter properly gates events before they reach the output layer
    // Note: OTel layers receive all events for distributed tracing/logging purposes
    let filtered_output_layer: BoxedLayer<tracing_subscriber::Registry> =
        if let Some(layer) = platform_layer {
            layer.with_filter(env_filter).boxed()
        } else {
            create_default_fmt_layer().with_filter(env_filter).boxed()
        };

    // Compose the output layer plus any OTel layers (traces/logs) into a single
    // subscriber, then init once. A global subscriber can only be initialized once.
    let mut layers: Vec<BoxedLayer<tracing_subscriber::Registry>> = vec![filtered_output_layer];

    let mut tracer_provider = None;
    let mut logger_provider = None;

    if cfg.tracing_enabled {
        let provider = build_otel_provider(cfg)?;
        let tracer = provider.tracer("actr-runtime");
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(otel_targets_filter(cfg))
            .boxed();
        layers.push(otel_layer);
        tracer_provider = Some(provider);
    }

    if cfg.logs_enabled {
        let provider = build_logs_provider(cfg)?;
        let log_layer = OpenTelemetryTracingBridge::new(&provider)
            .with_filter(otel_targets_filter(cfg))
            .boxed();
        layers.push(log_layer);
        logger_provider = Some(provider);
    }

    let _ = tracing_subscriber::registry().with(layers).try_init();

    Ok(ObservabilityGuard {
        tracer_provider,
        logger_provider,
    })
}

/// Create the default fmt layer for stderr output.
fn create_default_fmt_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    // Enable ANSI colors on Linux/Unix platforms for better terminal readability
    // Disable on mobile platforms (iOS/Android) where colors are not useful
    let enable_ansi = cfg!(all(
        unix,
        not(target_os = "ios"),
        not(target_os = "android")
    ));

    fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_level(true)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(enable_ansi)
}

#[cfg(feature = "opentelemetry")]
fn build_otel_provider(config: &ObservabilityConfig) -> ActorResult<SdkTracerProvider> {
    let tls = build_tls_config(config)?;
    let builder = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(apply_tls_scheme(&config.tracing_endpoint, tls.is_some()));
    let builder = match tls {
        Some(tls) => builder.with_tls_config(tls),
        None => builder,
    };
    let exporter = builder.build().map_err(|e| {
        actr_protocol::ActrError::Internal(format!("OTLP exporter build failed: {e}"))
    })?;

    let resource = Resource::builder()
        .with_service_name(config.tracing_service_name.clone())
        .with_attributes([KeyValue::new("telemetry.sdk.language", "rust")])
        .build();

    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    Ok(tracer_provider)
}

/// Build the OTLP logs provider (`SdkLoggerProvider`) exporting to the same OTLP
/// endpoint as traces. Only called when `cfg.logs_enabled` is true. The emitted
/// log records carry the same resource (service.name) as traces, so the backend
/// can correlate logs with spans.
#[cfg(feature = "opentelemetry")]
fn build_logs_provider(config: &ObservabilityConfig) -> ActorResult<SdkLoggerProvider> {
    let tls = build_tls_config(config)?;
    let builder = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(apply_tls_scheme(&config.tracing_endpoint, tls.is_some()));
    let builder = match tls {
        Some(tls) => builder.with_tls_config(tls),
        None => builder,
    };
    let exporter = builder.build().map_err(|e| {
        actr_protocol::ActrError::Internal(format!("OTLP logs exporter build failed: {e}"))
    })?;

    let resource = Resource::builder()
        .with_service_name(config.tracing_service_name.clone())
        .with_attributes([KeyValue::new("telemetry.sdk.language", "rust")])
        .build();

    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    Ok(logger_provider)
}

/// Build a tonic `ClientTlsConfig` from the configured CA cert, if any.
///
/// Returns `Ok(None)` when no `ca_cert_path` is configured (plaintext endpoint).
#[cfg(feature = "opentelemetry")]
fn build_tls_config(config: &ObservabilityConfig) -> ActorResult<Option<ClientTlsConfig>> {
    match &config.ca_cert_path {
        Some(path) => {
            let pem = std::fs::read(path).map_err(|e| {
                actr_protocol::ActrError::Internal(format!(
                    "failed to read OTLP CA cert '{}': {e}",
                    path.display()
                ))
            })?;
            let ca = Certificate::from_pem(pem);
            Ok(Some(ClientTlsConfig::new().ca_certificate(ca)))
        }
        None => Ok(None),
    }
}

/// When TLS is enabled, upgrade an `http://` OTLP endpoint to `https://`.
///
/// tonic only enables TLS for `https://` endpoints; an `http://` endpoint with a
/// TLS config silently falls back to plaintext and fails against a TLS-only
/// collector (broken pipe). This keeps `ca_cert_path`-style configs working
/// without requiring users to hand-write `https://` in the endpoint.
#[cfg(feature = "opentelemetry")]
fn apply_tls_scheme(endpoint: &str, tls_enabled: bool) -> String {
    match (tls_enabled, endpoint.strip_prefix("http://")) {
        (true, Some(rest)) => format!("https://{rest}"),
        _ => endpoint.to_string(),
    }
}

/// Target-level filter for OTel layers (traces + logs): use the configured
/// filter_level as default, and suppress noisy third-party crates.
#[cfg(feature = "opentelemetry")]
fn otel_targets_filter(cfg: &ObservabilityConfig) -> Targets {
    let default_level = cfg
        .filter_level
        .parse::<tracing::Level>()
        .unwrap_or(tracing::Level::INFO);
    Targets::new()
        .with_default(default_level)
        .with_target("tungstenite", tracing::Level::ERROR) // OFF equivalent
        .with_target("tokio_tungstenite", tracing::Level::ERROR) // OFF equivalent
        .with_target("wasmtime", tracing::Level::WARN)
        .with_target("rtc_mdns::conn", tracing::Level::WARN)
        .with_target("rtc_ice", tracing::Level::WARN)
        .with_target("rtc_sctp", tracing::Level::WARN)
}

#[cfg(test)]
#[path = "observability_tests.rs"]
mod tests;
