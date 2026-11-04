//! System metrics collection using pwrzv

use crate::error::{AdminError, Result};
use loran_proto::{ServiceStatus, SystemMetrics};
use std::sync::Arc;

/// Collect system metrics.
///
/// pwrzv 0.7 returns ratio-based metrics (0.0–1.0 for most dimensions).
/// We map the available ratios into the proto `SystemMetrics` as percentages
/// where possible; absolute byte counts are not available from pwrzv.
pub async fn collect_system_metrics() -> Result<SystemMetrics> {
    let (_, details) = pwrzv::get_power_reserve_level_with_details_direct()
        .await
        .map_err(|e| {
            platform::recording::warn!("Failed to read system metrics: {}", e);
            AdminError::Metrics(e.to_string())
        })?;

    let val = |key: &str| details.get(key).map(|d| d.value).unwrap_or(0.0);

    Ok(SystemMetrics {
        cpu_usage_percent: (val("cpu_usage") * 100.0) as f64,
        memory_usage_percent: (val("memory_usage") * 100.0) as f64,
        memory_used_bytes: 0, // pwrzv provides ratios only
        memory_total_bytes: 0,
        network_rx_bytes: 0,
        network_tx_bytes: 0,
        disk_used_bytes: 0,
        disk_total_bytes: 0,
        load_average_1m: val("cpu_load") as f64,
        load_average_5m: None,
        load_average_15m: None,
    })
}

/// Service status provider type (used for ReportRequest)
pub type ServiceStatusProviderForReport = Arc<dyn Fn() -> Vec<ServiceStatus> + Send + Sync>;

/// Collect service status.
///
/// If `service_status_provider` is provided, uses it to obtain service status;
/// otherwise returns an empty list (backward compatibility).
pub fn collect_service_status(
    service_status_provider: Option<ServiceStatusProviderForReport>,
) -> Vec<ServiceStatus> {
    if let Some(provider) = service_status_provider {
        provider()
    } else {
        // Backward compatibility: return empty list if no provider is given
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Depends on system environment; CI may fail
    async fn test_collect_metrics() {
        if let Ok(metrics) = collect_system_metrics().await {
            assert!(metrics.memory_total_bytes > 0);
            assert!(metrics.cpu_usage_percent >= 0.0);
        }
    }
}
