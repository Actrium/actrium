# TODO - Production Readiness

距离工业级远程桌面 (类似 MS RDP/TSC) 的技术差距。

## Priority 1: 安全性 (Critical)

### 1.1 身份认证
- [ ] ConnectRequest 添加密码/Token 验证
- [ ] Challenge-response 认证机制
- [ ] 连接前验证，而非连接后

**当前问题**: `host/src/actr/workload.rs:250` 仅生成 UUID，无认证
```rust
let new_session_id = uuid::Uuid::new_v4().to_string();
```

### 1.2 端到端加密
- [x] WebRTC DTLS 传输层加密 (已有)
- [ ] 应用层加密 (可选，如需双重保护)

**说明**: WebRTC 已使用 DTLS，满足大多数场景

### 1.3 密码存储安全
- [ ] 修复 `libs/hbb_common/src/password_security.rs` 固定 nonce 问题

```rust
// 当前问题：固定 nonce 削弱加密安全性
let nonce = secretbox::Nonce([0; secretbox::NONCEBYTES]);
```

---

## Priority 2: 网络连通性

### 2.1 STUN/TURN (已配置)
- [x] STUN 服务器 - Actr.toml 已配置
- [x] TURN 服务器 - Actr.toml 已配置

**配置位置**: `host/Actr.toml` 和 `client/Actr.toml`
```toml
[system.webrtc]
stun_urls = ["stun:loran1.actrium-dev.example:3478"]
turn_urls = ["turn:loran1.actrium-dev.example:3478"]
force_relay = false
```

### 2.2 连接可靠性
- [ ] ACTR 层断线重连逻辑
- [ ] RPC 调用重试（指数退避）
- [ ] 心跳超时检测和会话清理

**当前问题**: `host/src/actr/workload.rs` Heartbeat RPC 存在但无超时清理

---

## Priority 3: 跨平台支持

### 3.1 Windows 虚拟显示器
- [ ] 使用 Indirect Display Driver (IDD) 实现
- [ ] 参考: https://github.com/MolotovCherry/virtual-display-rs

### 3.2 Linux 虚拟显示器
- [ ] 使用 DRM/KMS 或 X11 虚拟输出
- [ ] 参考: xrandr --addmode / xdummy

### 3.3 Windows/Linux 隐私模式
- [ ] Windows: 使用 Display Driver 黑屏
- [ ] Linux: 使用 XRandR 或 DRM

**当前状态**: 仅 macOS 支持虚拟显示器和隐私模式

---

## Priority 4: 性能优化

### 4.1 自适应码率
- [ ] 将 `host/src/server/video_qos.rs` 集成到 ACTR 协议
- [ ] 客户端报告网络延迟，Host 动态调整码率

### 4.2 硬件编解码
- [ ] 默认启用 hwcodec feature（如果可用）
- [ ] 添加编解码能力协商

---

## Priority 5: 用户体验

### 5.1 完整 GUI 客户端
- [ ] 连接管理界面
- [ ] 设置界面
- [ ] 连接历史

### 5.2 配置管理
- [ ] 图形化配置界面
- [ ] 导出/导入配置

---

## 已完成功能

- [x] ACTR WebRTC 通信
- [x] STUN/TURN NAT 穿透
- [x] VP9/VP8 视频编解码
- [x] Opus 音频编解码
- [x] 键盘/鼠标输入
- [x] 剪贴板同步
- [x] 文件传输
- [x] 虚拟显示器 (macOS)
- [x] 隐私模式 (macOS)
- [x] 原生 Cocoa UI (macOS)

---

## 技术架构

### ACTR 连接流程
```
Client                 Actrix Server              Host
   |                        |                       |
   |-- Discover Host ------>|                       |
   |<-- Host Address -------|                       |
   |                        |                       |
   |-- SDP Offer (via Actrix) -------------------->|
   |<-- SDP Answer (via Actrix) -------------------|
   |                        |                       |
   |<========== WebRTC P2P (DTLS encrypted) ======>|
   |            (via STUN or TURN if needed)        |
```

### ICE 连通性
1. **STUN**: 发现公网 IP，尝试 P2P 直连
2. **TURN**: P2P 失败时作为中继（已配置）

### 关键文件
| 文件 | 说明 |
|------|------|
| `host/Actr.toml` | Host ACTR 配置 (信令、ICE) |
| `client/Actr.toml` | Client ACTR 配置 |
| `host/src/actr/workload.rs` | Host 业务逻辑 |
| `client/src/actr/workload.rs` | Client 业务逻辑 |
| `host/protos/rustdesk_actr.proto` | 协议定义 |

---

## 代码清理 (已完成)

### 遗留传输层代码 (libs/hbb_common/)

已清理或 stub 的模块：

| 模块 | 状态 |
|------|------|
| `webrtc.rs` | ✅ 已删除 |
| `tcp.rs` | ✅ 已替换为 stub |
| `udp.rs` | ✅ 已删除 |
| `socket_client.rs` | ✅ 已替换为 stub |
| `proxy.rs` | ✅ 已删除 |
| `websocket.rs` | ✅ 已删除 |
| `stream.rs` | ✅ 已替换为 stub |
| `tls.rs` | ✅ 已替换为 stub |
| `rendezvous.rs` | ✅ 已删除 |

### 遗留代码处理
- `host/src/server/mod.rs` - 遗留 TCP 连接函数已 stub 化
- `client/src/common.rs` - 遗留 NAT 测试函数已 stub 化
- `client/src/ui_interface.rs` - 遗留 ID 变更函数已 stub 化

**说明**: ACTR 使用 WebRTC 传输，不依赖遗留传输层。保留的 stub 仅为 API 兼容性。
