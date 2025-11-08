//! System metrics collection using pwrzv

use crate::error::{Result, SupervitError};
use crate::generated::{ServiceStatus, SystemMetrics};
use tracing::warn;

/// 收集系统指标
pub async fn collect_system_metrics() -> Result<SystemMetrics> {
    let (_, details) = pwrzv::get_power_reserve_level_with_details_direct()
        .await
        .map_err(|e| {
            warn!("Failed to read system metrics: {}", e);
            SupervitError::Metrics(e.to_string())
        })?;

    // 从详细信息中提取指标 (pwrzv 返回 HashMap<String, f32>)
    let cpu_usage = details.get("cpu_usage").copied().unwrap_or(0.0) as f64;
    let memory_used = details.get("memory_used").copied().unwrap_or(0.0) as u64;
    let memory_total = details.get("memory_total").copied().unwrap_or(0.0) as u64;
    let load_avg_1 = details.get("load_avg_1").copied().unwrap_or(0.0) as f64;
    let load_avg_5 = details.get("load_avg_5").copied().unwrap_or(0.0) as f64;
    let load_avg_15 = details.get("load_avg_15").copied().unwrap_or(0.0) as f64;

    Ok(SystemMetrics {
        cpu_usage_percent: cpu_usage,
        memory_used_bytes: memory_used,
        memory_total_bytes: memory_total,
        memory_usage_percent: if memory_total > 0 {
            (memory_used as f64 / memory_total as f64) * 100.0
        } else {
            0.0
        },
        network_rx_bytes: 0, // pwrzv 0.6 不提供网络统计
        network_tx_bytes: 0,
        disk_used_bytes: 0, // pwrzv 不提供磁盘统计
        disk_total_bytes: 0,
        load_average_1m: load_avg_1,
        load_average_5m: Some(load_avg_5),   // proto2 optional 字段
        load_average_15m: Some(load_avg_15), // proto2 optional 字段
    })
}

/// 收集服务状态（需要从 actrix 服务管理器获取）
pub fn collect_service_status() -> Vec<ServiceStatus> {
    // TODO: 从 ServiceManager 获取真实服务状态
    // 当前返回空列表，后续集成时实现
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 依赖系统环境，CI 可能失败
    async fn test_collect_metrics() {
        if let Ok(metrics) = collect_system_metrics().await {
            assert!(metrics.memory_total_bytes > 0);
            assert!(metrics.cpu_usage_percent >= 0.0);
        }
    }
}
