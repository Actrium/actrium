# Actrix 配置示例

本文档提供各种部署场景的配置示例。

## 📋 目录

1. [单机全服务部署](#单机全服务部署)
2. [分布式部署](#分布式部署)
3. [多区域部署](#多区域部署)
4. [智能 KS 默认配置](#智能-ks-默认配置)

---

## 单机全服务部署

**场景**: 在一台机器上运行所有服务（开发/测试环境）

```toml
name = "actrix-dev"
env = "dev"
actrix_shared_key = "dev-shared-key-change-in-production"

# 启用 STUN + TURN（位掩码）
enable = 6

# KS 服务
[services.ks]
enabled = true

[services.ks.storage]
backend = "sqlite"
key_ttl_seconds = 3600

[services.ks.storage.sqlite]
path = "ks.db"

# AIS 服务（自动使用本地 KS）
[services.ais]
enabled = true

[services.ais.server]
database_path = "ais.db"

# 📝 注意：AIS 没有配置 dependencies.ks
# 它会自动发现本地 KS 并通过 gRPC 连接

# Signaling 服务（可选，也会自动使用本地 KS）
[services.signaling]
enabled = true

[services.signaling.server]
ws_path = "/signaling"

[bind.https]
ip = "0.0.0.0"
port = 8443
cert = "certificates/server.crt"
key = "certificates/server.key"

[turn]
advertised_ip = "127.0.0.1"
realm = "actrix.local"
```

---

## 分布式部署

### 场景 A: 专用 KS 服务器

**KS 服务器** (`ks-server.toml`)

```toml
name = "actrix-ks"
env = "prod"
actrix_shared_key = "PROD_SHARED_KEY_32_CHARS_MINIMUM"

# 只启用 KS 服务
enable = 0

[services.ks]
enabled = true

[services.ks.storage]
backend = "sqlite"
key_ttl_seconds = 7200  # 2小时

[services.ks.storage.sqlite]
path = "/var/lib/actrix/ks.db"

[bind.https]
ip = "0.0.0.0"
port = 8443
cert = "/etc/actrix/tls/ks-cert.pem"
key = "/etc/actrix/tls/ks-key.pem"
```

### 场景 B: 业务服务器（连接远程 KS）

**业务服务器** (`business-server.toml`)

```toml
name = "actrix-business-01"
env = "prod"
actrix_shared_key = "PROD_SHARED_KEY_32_CHARS_MINIMUM"  # 与 KS 相同

# 启用 STUN + TURN
enable = 6

# 本地不运行 KS
# services.ks 未配置

# AIS 服务（连接远程 KS）
[services.ais]
enabled = true

[services.ais.server]
database_path = "/var/lib/actrix/ais.db"

# 显式配置远程 KS（gRPC endpoint）
[services.ais.dependencies.ks]
endpoint = "https://ks.internal.example.com:50052"
timeout_seconds = 10

# Signaling 服务（连接相同的远程 KS）
[services.signaling]
enabled = true

[services.signaling.server]
ws_path = "/signaling"

[services.signaling.dependencies.ks]
endpoint = "https://ks.internal.example.com:50052"
timeout_seconds = 5

[bind.https]
ip = "0.0.0.0"
port = 8443
cert = "/etc/actrix/tls/business-cert.pem"
key = "/etc/actrix/tls/business-key.pem"

[turn]
advertised_ip = "203.0.113.10"  # 公网 IP
realm = "actrix.example.com"
```

---

## 多区域部署

### 区域 A: 美西（使用美西 KS）

```toml
name = "actrix-us-west-01"
location_tag = "aws,us-west-2,zone-a"
actrix_shared_key = "SHARED_KEY"

[services.ais]
enabled = true

[services.ais.dependencies.ks]
endpoint = "https://ks-us-west.internal:50052"
timeout_seconds = 10
```

### 区域 B: 欧洲（使用欧洲 KS）

```toml
name = "actrix-eu-central-01"
location_tag = "aws,eu-central-1,zone-a"
actrix_shared_key = "SHARED_KEY"

[services.ais]
enabled = true

[services.ais.dependencies.ks]
endpoint = "https://ks-eu-central.internal:50052"
timeout_seconds = 10
```

---

## 智能 KS 默认配置

### 工作原理

```
┌─────────────────────────────────────────────────────────┐
│          智能 KS 客户端配置优先级                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  1️⃣  显式配置 (services.*.dependencies.ks)              │
│     ↓ 如果存在，直接使用                                 │
│                                                         │
│  2️⃣  本地 KS 自动发现                                    │
│     ↓ 如果 services.ks.enabled = true                  │
│     ↓ 自动生成: http://127.0.0.1:{ks_port}             │
│                                                         │
│  3️⃣  配置错误                                           │
│     ↓ 对于 AIS（必需 KS）：返回错误                      │
│     ↓ 对于 Signaling（可选 KS）：返回 None              │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 示例 1: 最小配置（推荐）

```toml
# ✅ 最简洁的配置 - 自动发现本地 KS
actrix_shared_key = "shared-key"

[services.ks]
enabled = true

[services.ais]
enabled = true
# 不需要配置 dependencies.ks
# AIS 自动通过 gRPC 连接本地 KS (http://127.0.0.1:50052)
```

**等价于**:

```toml
[services.ais]
enabled = true

[services.ais.dependencies.ks]
endpoint = "http://127.0.0.1:50052"  # gRPC 端口
timeout_seconds = 30
```

### 示例 2: 显式配置覆盖自动发现

```toml
[services.ks]
enabled = true  # 本地 KS 在运行

[services.ais]
enabled = true

# 显式配置优先级更高
[services.ais.dependencies.ks]
endpoint = "http://remote-ks:50052"  # 连接远程 KS，忽略本地
timeout_seconds = 15
```

### 示例 3: 不同服务使用不同 KS

```toml
[services.ks]
enabled = true

[services.ais]
enabled = true
# AIS 使用本地 KS（自动发现）
# dependencies.ks 未配置

[services.signaling]
enabled = true
# Signaling 使用远程 KS（显式配置）

[services.signaling.dependencies.ks]
endpoint = "http://backup-ks:50052"
timeout_seconds = 10
```

### 示例 4: 验证配置

```bash
# 测试配置有效性
cargo run --bin actrix -- test --config config.toml

# 成功示例：
# ✅ KS service is enabled
# ✅ AIS service will use KS at http://127.0.0.1:8090 (auto-discovered)
# ✅ Configuration is valid

# 错误示例：
# ❌ AIS service is enabled but no KS available:
#    either configure services.ais.dependencies.ks or enable local KS service
```

---

## 配置验证规则

### AIS 服务（必需 KS）

```toml
# ❌ 错误配置 - AIS 需要 KS
[services.ais]
enabled = true
# 既没有本地 KS，也没有显式配置

# ✅ 正确配置 - 方式 1：本地 KS
[services.ks]
enabled = true

[services.ais]
enabled = true

# ✅ 正确配置 - 方式 2：显式配置远程 KS
[services.ais]
enabled = true

[services.ais.dependencies.ks]
endpoint = "http://remote-ks:50052"
```

### Signaling 服务（可选 KS）

```toml
# ✅ 可以不依赖 KS
[services.signaling]
enabled = true
# 不配置 dependencies.ks 也可以运行

# ✅ 如果需要加密，可以配置 KS
[services.signaling]
enabled = true

[services.signaling.dependencies.ks]
endpoint = "http://ks:50052"
```

---

## 最佳实践

### 1. **开发环境**
- ✅ 使用自动发现（不配置 dependencies.ks）
- ✅ 所有服务运行在 localhost
- ✅ 简化配置，快速启动

### 2. **生产环境**
- ✅ 显式配置所有 KS 端点
- ✅ 使用 HTTPS 连接
- ✅ 配置独立的 cache_db_path
- ✅ 设置合理的 timeout

### 3. **高可用部署**
- ✅ 使用负载均衡器作为 KS endpoint
- ✅ 配置多个业务服务器连接同一 KS 集群
- ✅ 监控 KS 连接状态

### 4. **安全建议**
- ✅ 使用强 `actrix_shared_key`（≥32 字符）
- ✅ 定期轮换密钥
- ✅ 生产环境使用 HTTPS
- ✅ 限制 KS 服务的网络访问

---

## 故障排查

### Q: AIS 启动失败，提示找不到 KS

**错误信息**:
```
AIS service is enabled but no KS available
```

**解决方案**:
1. 检查是否启用了本地 KS：`services.ks.enabled = true`
2. 或者显式配置远程 KS：`services.ais.dependencies.ks`

### Q: AIS 连接了错误的 KS

**问题**: 配置了远程 KS，但 AIS 仍然连接本地

**原因**: 显式配置的优先级最高，检查配置文件中是否有 `services.ais.dependencies.ks` 段落

### Q: 如何查看 AIS 使用的 KS 端点

**方法**:
```bash
# 方式 1: 查看启动日志
tail -f logs/actrix.log | grep "KS endpoint"

# 方式 2: 运行配置测试
cargo run --bin actrix -- test --config config.toml
```

---

## 参考

- [配置文件参考](./CONFIGURATION.md)
- [KS 完全指南](./KS_GUIDE.md)
- [服务架构](./SERVICES.md)
