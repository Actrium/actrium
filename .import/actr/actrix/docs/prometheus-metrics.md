# Prometheus 监控指标实现

## 概述

本文档描述了 Actrix-GW1 项目中 Prometheus 监控指标的实现方式和使用指南。

## 架构设计

### 全局 Registry

- 位置: `crates/common/src/metrics.rs`
- 全局 Registry: `actrix_common::metrics::REGISTRY`
- 初始化: 在 `src/main.rs` 启动时自动注册所有基础指标

### 指标分类

#### 1. 业务指标
- `actrix_actors_total`: 当前注册的 Actor 总数（按 realm 分组）
- `actrix_services_total`: 当前注册的服务总数
- `actrix_websocket_connections`: WebSocket 连接数
- `actrix_tokens_issued_total`: Token 颁发次数
- `actrix_tokens_validated_total`: Token 验证次数

#### 2. 性能指标
- `actrix_request_duration_seconds`: HTTP 请求延迟（Histogram）
  - 桶边界: [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0] 秒
  - 标签: service, method, path, status
- `actrix_requests_total`: HTTP 请求总数（Counter）
- `actrix_errors_total`: 错误次数

#### 3. 安全指标
- `actrix_rate_limit_exceeded_total`: 速率限制触发次数
- `actrix_auth_failures_total`: 认证失败次数
  - 标签: service, reason (replay_attack, invalid_signature, unknown)
- `actrix_invalid_requests_total`: 非法请求次数

#### 4. KS 服务特定指标
- `actrix_keys_generated_total`: 密钥生成次数
  - 标签: key_type (ecies)
- `actrix_key_rotations_total`: 密钥轮转次数

#### 5. TURN 服务特定指标
- `actrix_turn_allocations_total`: TURN 分配请求
- `actrix_turn_active_sessions`: TURN 活跃会话数
- `actrix_turn_bytes_relayed_total`: TURN 中继流量统计

## 服务集成模式

### 方式一：使用全局 Metrics（避免循环依赖）

由于 `actrix-common` 依赖 `ks`，为避免循环依赖，各服务采用独立定义 metrics 的方式。

**示例：KS 服务**

```rust
// crates/ks/src/handlers.rs
use lazy_static::lazy_static;
use prometheus::{HistogramVec, IntCounterVec, ...};

lazy_static! {
    static ref KS_KEYS_GENERATED: IntCounterVec = IntCounterVec::new(
        Opts::new("actrix_keys_generated_total", "...").namespace("actrix"),
        &["key_type"]
    ).unwrap();

    static ref KS_REQUEST_DURATION: HistogramVec = ...;
}

pub fn register_ks_metrics(registry: &prometheus::Registry) -> Result<(), prometheus::Error> {
    registry.register(Box::new(KS_KEYS_GENERATED.clone()))?;
    registry.register(Box::new(KS_REQUEST_DURATION.clone()))?;
    Ok(())
}
```

**在启动时注册：**

```rust
// src/main.rs
if config.is_ks_enabled() {
    ks::register_ks_metrics(&actrix_common::metrics::REGISTRY)?;
}
```

### 方式二：在处理器中记录指标

```rust
async fn generate_key_handler(...) -> Result<...> {
    let start_time = Instant::now();

    // 业务逻辑
    let result = do_work().await?;

    // 记录指标
    KS_KEYS_GENERATED.with_label_values(&["ecies"]).inc();

    let duration = start_time.elapsed().as_secs_f64();
    KS_REQUEST_DURATION
        .with_label_values(&["ks", "POST", "/generate", "200"])
        .observe(duration);
    KS_REQUESTS_TOTAL
        .with_label_values(&["ks", "POST", "/generate", "200"])
        .inc();

    Ok(result)
}
```

## /metrics 端点

### 访问方式

```bash
# HTTP (开发环境)
curl http://localhost:8080/metrics

# HTTPS (生产环境)
curl https://your-domain:8443/metrics
```

### 输出示例

```
# HELP actrix_keys_generated_total Total number of keys generated
# TYPE actrix_keys_generated_total counter
actrix_keys_generated_total{key_type="ecies"} 42

# HELP actrix_request_duration_seconds HTTP request duration in seconds
# TYPE actrix_request_duration_seconds histogram
actrix_request_duration_seconds_bucket{service="ks",method="POST",path="/generate",status="200",le="0.001"} 10
actrix_request_duration_seconds_bucket{service="ks",method="POST",path="/generate",status="200",le="0.005"} 35
...
actrix_request_duration_seconds_sum{service="ks",method="POST",path="/generate",status="200"} 0.125
actrix_request_duration_seconds_count{service="ks",method="POST",path="/generate",status="200"} 42
```

## Prometheus 配置

### scrape_configs 示例

```yaml
scrape_configs:
  - job_name: 'actrix-gw1'
    static_configs:
      - targets: ['actrix-gw1:8443']
    scheme: https
    tls_config:
      insecure_skip_verify: true  # 仅用于内部自签名证书
    scrape_interval: 15s
    scrape_timeout: 10s
```

### Grafana Dashboard 查询示例

**1. 请求速率**
```promql
rate(actrix_requests_total{service="ks"}[5m])
```

**2. P95 延迟**
```promql
histogram_quantile(0.95,
  rate(actrix_request_duration_seconds_bucket{service="ks"}[5m])
)
```

**3. 错误率**
```promql
sum(rate(actrix_requests_total{status=~"4..|5.."}[5m]))
/
sum(rate(actrix_requests_total[5m]))
```

**4. 密钥生成速率**
```promql
rate(actrix_keys_generated_total[5m])
```

**5. 认证失败趋势**
```promql
rate(actrix_auth_failures_total[5m]) by (reason)
```

## 当前实现状态

### ✅ 已完成
- [x] 基础 metrics 模块 (`actrix-common/src/metrics.rs`)
- [x] 全局 /metrics HTTP 端点
- [x] KS 服务完整集成
  - 密钥生成计数
  - HTTP 请求延迟和计数
  - 认证失败统计
  - 错误跟踪

### 🔄 待完成
- [ ] AIS 服务集成
  - Token 颁发计数
  - 请求延迟统计
- [ ] Signaling 服务集成
  - WebSocket 连接数
  - Actor 注册统计
  - 速率限制触发统计
- [ ] TURN 服务集成
  - 分配请求统计
  - 活跃会话数
  - 流量统计

## 集成指南（其他服务）

### 步骤 1: 添加依赖

```toml
# crates/your-service/Cargo.toml
[dependencies]
prometheus = "0.13"
lazy_static = "1.4"
```

### 步骤 2: 定义 Metrics

```rust
// crates/your-service/src/handlers.rs
use lazy_static::lazy_static;
use prometheus::{IntCounterVec, Opts};

lazy_static! {
    static ref YOUR_METRIC: IntCounterVec = IntCounterVec::new(
        Opts::new("actrix_your_metric", "Description")
            .namespace("actrix"),
        &["label1", "label2"]
    ).unwrap();
}

pub fn register_your_service_metrics(registry: &prometheus::Registry)
    -> Result<(), prometheus::Error>
{
    registry.register(Box::new(YOUR_METRIC.clone()))?;
    Ok(())
}
```

### 步骤 3: 导出注册函数

```rust
// crates/your-service/src/lib.rs
pub use handlers::register_your_service_metrics;
```

### 步骤 4: 启动时注册

```rust
// src/main.rs
if config.is_your_service_enabled() {
    your_service::register_your_service_metrics(&actrix_common::metrics::REGISTRY)?;
}
```

### 步骤 5: 在业务逻辑中记录

```rust
async fn your_handler(...) -> Result<...> {
    let start = Instant::now();

    // 业务逻辑
    let result = process().await?;

    // 记录指标
    YOUR_METRIC.with_label_values(&["value1", "value2"]).inc();

    Ok(result)
}
```

## 最佳实践

### 1. 标签使用
- ✅ 使用低基数标签（service, method, status）
- ❌ 避免高基数标签（user_id, request_id, timestamp）

### 2. Histogram 桶设计
- 根据实际延迟分布调整桶边界
- 当前桶: [1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s]
- 适用于大多数 HTTP 请求场景

### 3. 错误处理
```rust
// ✅ 好的做法：区分错误类型
AUTH_FAILURES.with_label_values(&["service", "replay_attack"]).inc();
AUTH_FAILURES.with_label_values(&["service", "invalid_signature"]).inc();

// ❌ 避免：丢失错误上下文
ERRORS_TOTAL.with_label_values(&["service"]).inc();
```

### 4. 性能考虑
- Metrics 操作是线程安全的，有轻微开销
- 对于高频路径（>1000 QPS），考虑采样
- 使用 lazy_static 确保单例模式

## 监控告警建议

### 1. 可用性告警
```yaml
- alert: HighErrorRate
  expr: |
    sum(rate(actrix_requests_total{status=~"5.."}[5m]))
    /
    sum(rate(actrix_requests_total[5m])) > 0.05
  for: 5m
  annotations:
    summary: "Error rate > 5% for 5 minutes"
```

### 2. 性能告警
```yaml
- alert: HighLatency
  expr: |
    histogram_quantile(0.95,
      rate(actrix_request_duration_seconds_bucket[5m])
    ) > 1.0
  for: 10m
  annotations:
    summary: "P95 latency > 1s for 10 minutes"
```

### 3. 安全告警
```yaml
- alert: HighAuthFailureRate
  expr: |
    rate(actrix_auth_failures_total[5m]) > 10
  for: 5m
  annotations:
    summary: "Auth failures > 10/s, possible attack"
```

## 参考资料

- [Prometheus 最佳实践](https://prometheus.io/docs/practices/naming/)
- [Rust prometheus crate](https://docs.rs/prometheus/)
- [Histogram vs Summary](https://prometheus.io/docs/practices/histograms/)
