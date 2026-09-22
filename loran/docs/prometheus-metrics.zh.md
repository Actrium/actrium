# Loran Realm v1 指标

Loran 暴露两类互补观测数据：

- `/metrics` 与 `/{service}/metrics`：Prometheus 文本格式；
- `/admin/api/metrics/timeseries`：Admin UI 使用的单节点内存 ring buffer。

准确指标名以 `crates/loran-platform/src/metrics.rs` 和各 service 的 metrics 定义为准；
本文不复制一份可能过期的完整列表。

## 访问控制

生产环境应在 `[monitoring]` 配置 CIDR allowlist、htpasswd，或在受信反向代理后
限制访问。指标可能泄露 Actor 数量、连接量、错误模式和服务拓扑，不应默认公网开放。

```bash
curl --fail --cacert /etc/actrix/pki/ca.pem \
  https://realm.example.com/metrics
```

服务名使用 `signer`（Issuer Signer）、`issuer`、`signaling`、`stun`、`turn`。Publisher
Registry 目前没有独立 ring series；其 HTTP/安全事件仍进入全局指标与 recording channels。

## 命名与标签

- 新指标沿用代码中的 `actrix_` 前缀；
- Issuer Signer 的 label 统一为 `signer`；
- Realm、ActorId、Publisher、Actor credential、nonce 或 secret 不得成为高基数/敏感 label；
  目标契约下一个实例承载大量 Realm，Realm 名与域名尤其不得作为 label，按
  Realm 的状态、配额与挂起情况只以聚合计数暴露；
- path label 必须模板化，不能包含用户输入或 ID；
- 认证失败只记录有限枚举 reason。

## Ring buffer

单节点 dashboard 使用三层固定容量 ring，进程重启后为空，不做跨节点聚合或持久化。
详细设计见 [`design-metrics-ring.md`](design-metrics-ring.md)。

## 发布证据

指标存在不等于容量达标。性能声明必须附目标硬件、fixture、原始输出、P50/P95/P99、
错误率与资源峰值，见 [`../benchmarks/README.md`](../benchmarks/README.md)。
