# PR #542 尾款修复方案：p08（恢复交换合并）与 mp06（优雅关闭 retire）

两个缺陷同源：**模型切换时丢失了隐式契约**。修复原则一致——把丢掉的契约显式补回，
不引入新机制。

- p08：`129d6fd7`（08-17）把恢复决策搬进 per-peer reducer 时，丢掉了 main 0.14 恢复
  管理器 `begin()` 的 **`Existing` 收敛语义**（迟到事件被吸收，不另起恢复轮次）。
- mp06：`9a9482e4`（08-13）引入精确 retire 模型时，没有为**优雅关闭**这个终态补发布
  事件（发布条件只覆盖失败性终态）。

---

## 方案一（先做）：mp06 — 优雅关闭发布 transport retirement

### 根因回顾

```
close_all/close_peer → reduce_close: phase→Closing→Idle, unavailable_reason=None
  ↓ （永不满足 controller.rs:902 的发布条件 terminally_unavailable() && reason.is_some()）
不发布 peer_transport_retired
  ↓
PeerTransport.transports 残留 Connected(旧 DestTransport)
  ↓
显式 connect 只在 manager 层建新 session；PeerTransport 不知情
  ↓
RPC: get_or_create_transport 复用死 transport → send 挂 15s
  → "Request timed out after send without wire identity"
```

### 不变量（验收标准）

- **I-4**：每个逻辑传输终态——失败性（NegotiationFailed 等）**和优雅性（close）**——
  都产生一个 retirement 事件。
- **I-5**：retirement 按 lifecycle（controller_lifecycle_id + transport_epoch）精确匹配，
  不得误删 fresh connect 的新条目（既有 `close_transport_if_webrtc_lifecycle` 已保证）。
- **I-6**：优雅关闭时 pending requests 同样被释放（PeerGate retire 任务已有 drain 逻辑，
  补上事件后自然生效，需测试确认）。

### 改动点（预计 ≤ 3 处，均在 `core/hyper/src/wire/webrtc/v020/peer/`）

1. **`RtcUnavailableReason` 增加变体** `Closed`（优雅关闭语义）。
   内部枚举（测试断言 `reason=NegotiationFailed` 所见），加变体无协议影响。

2. **`reducer.rs` `reduce_close`**：在关闭收敛点设置 reason，让现有发布条件自然满足：
   - `terminal_close_done = true` 的两个收敛点（`maybe_finish_close` 收敛处，以及
     "explicit close of a freshly-created/idle controller" 的 early-return 分支）：
     `state.unavailable_reason = Some(RtcUnavailableReason::Closed);`
   - 收敛时 phase 已为 Idle、active/pending/building_generation 已全 None →
     `terminally_unavailable()` 为真 → controller 的既有边沿发布
     （`!was_terminally_unavailable && now_terminally_unavailable`）恰好触发一次。
   - 不动 controller.rs 的发布条件本身——"终态 = 有 reason"的模型保持统一。

3. **（如缺）`controller.rs` close 命令的 reduce 调用后触发 snapshot 发布路径**：
   确认 close 收敛的那次 reduce 走到 `:902` 的发布块（现有 close 路径每次 reduce 都
   publish snapshot，应天然覆盖；实施时用单测验证）。

### 为什么不选其它方案

- ❌ 在 `close_all`/`close_peer`（manager 层）直接调 `PeerTransport::close_transport`：
  绕过 lifecycle 精确匹配，可能误删并发新连接；且 manager 不持有 PeerTransport。
- ❌ PeerGate 轮询 dest_count 对账：违背事件驱动模型，治标。

### 测试计划

- 单测（reducer/controller_tests）：
  - close 收敛后 `unavailable_reason == Some(Closed)` 且 `terminally_unavailable()`；
  - deps 的 `peer_transport_retired` 通道收到事件（lifecycle/transport_epoch 正确）；
  - fresh-created/idle controller 的 close early-return 分支同样发布。
- 集成：
  - `mp06`（本地稳定复现）转绿；
  - 回归：`mp05`、`p18`、`node_start_smoke::peer_disconnect_triggers_coordinator_cleanup`、
    `network_event_handling::test_manual_cleanup_connections`（其 `dest_count()==0` 断言
    在此修复后才真正有事件链支撑）。
  - 双路径验证：close→立即 connect→RPC（mp06）；close→不 connect（残留被清）。

### 风险与边界

- **重复发布**：现有条件本身是 `!was && is` 边沿语义，close 只收敛一次，天然只发一次。
- **p18 行为变化**：其旧条目此前未被清（侥幸绿），修复后走全新 transport flight——
  更正确，靠回归测试兜底。
- **消费者确认**：`PeerTransportRetirement` 仅 PeerGate 清理任务消费（peer_gate.rs:160），
  不进 supervisor fact 通道，不影响 #568 状态机。

---

## 方案二（后做）：p08 — 延迟恢复合并进在飞交换

### 根因回顾

```
B 检测断链 → IceRestartRequest → A 开启 restart 交换（exchange="v020-1-1-10"，
  recovery_attempt_started=true, ice_generation=1, 协商在飞）
A.background() → AppBackground arm → defer_recovery_until_gate()
  └─ 无条件 clear_negotiation_for_recovery()：
       pending_local_restart_exchange_id=None（交换身份被抹）
       recovery_attempt_started=false（"这轮没开始过"）
A.foreground() → maybe_start_deferred_recovery → start_soft_recovery
  → ice_generation+1 → 新一轮交换 "v020-1-2-13"          ← 契约要求合并，✗
```

### 不变量（吸收 main `begin()` 三语义，验收标准）

- **I-1（合并，对应 main `Existing`）**：同一恢复轮次内，迟到的生命周期/平台事件
  不得产生新交换——同 `ice_generation`、同 exchange id 重发。
- **I-2（deadline 不延长，对应 main `Retriggered` 原语）**：暂停期间 deadline 继续走，
  重发不刷新。
- **I-3（活/死分治）**：活着的交换 → 合并（I-1）；已被 deadline 判死且消息未送达 →
  保留 1b18ee87 的 resurrect 路径（begin_transport_attempt）。两条路径互斥，
  消除 resurrect/coalesce 冲突。

### 改动点（均在 `core/hyper/src/wire/webrtc/v020/peer/reducer.rs`）

1. **协商在飞判定**（新 helper）：
   ```rust
   fn restart_negotiation_inflight(state: &RtcPeerState) -> bool {
       state.recovery_attempt_started && (
           // offerer：restart offer 交换已建立、未完成
           state.pending_local_restart_exchange_id.is_some()
           // answerer：restart request 已发出、在等 offer
           || state.local_restart_request_id.is_some()
       )
   }
   ```

2. **`defer_recovery_until_gate(state, hard)`**：在飞时走"暂停"分支——
   - **不**调 `clear_negotiation_for_recovery`（保住交换身份）；
   - **不**重置 `recovery_attempt_started`；
   - **不** bump `ice_generation`；
   - **不** bump `recovery_epoch`（deadline 继续有效 → I-2）；
   - 仍置 `phase=Recovering`、`pc_connected=false`（S5-5 的"后台时 sendable 必须过期"
     契约保留）；
   - 记 `recovery_paused = true`（新字段，或以 `recovery_attempt_started && !gate_open`
     推导——实施时按状态最小侵入选择）。

3. **恢复边沿（`AppForeground` arm / `SignalingGenerationChanged(Some)` 的
   `maybe_start_deferred_recovery`）**：先查暂停且在飞 → `resume`：
   - offerer：以**同一** `pending_local_restart_exchange_id` 重发 IceRestartOffer；
   - answerer：`send_answerer_restart_request`（已复用 `local_restart_request_id`、
     attempt 计数 +1）——现成机制；
   - `recovery_paused=false`；ice_generation 不动。
   - gate 仍关时 resume 依旧被 `recovery_gate_open` 拦住（既有检查），消息不会白发。

4. **deadline 与暂停的交互**：暂停期间 RoleNegotiation/RecoveryDeadline 到点 →
   正常判死（NegotiationFailed）；若被判死消息未送达（gate 关闭期发出被扔）→
   标记 undelivered（与 `role_probe_undelivered` 同构，实施时确认 restart offer
   是否需要等价的 `restart_offer_undelivered`，或统一为单一标记）→ gate 开启边沿
   走既有 resurrect。活→合并、死→复活，互斥。

### 测试计划

- reducer 单测（新增 4 个）：
  1. background 时 restart 协商在飞 → 状态保留（exchange id 不变、attempt_started
     保持、ice_generation 不变、epoch 不 bump）；
  2. foreground → effects 里重发**同一 exchange id** 的 IceRestartOffer，无新交换；
  3. 暂停期间 deadline 判死 → foreground + gate 开 → resurrect（对齐 1b18ee87 既有
     测试期望，确认二者不冲突）；
  4. 非 restart 场景（Sendable→background→foreground）行为不变（回归）。
- 集成：`p08` 转绿；回归 `mobile_rtc_pair_contract` 全套、`ice_restart`、
  `ice_restart_with_signaling_disruption`、`mobile_full_disconnect_recovery`、
  `rapid_dedup`（32 次抖动不再每次清协商，预期同步受益）。

### 风险与边界

- **对端已死**：暂停的交换重发后无应答 → deadline 判死 → 正常升级路径，可接受。
- **与 close 竞态**：`reduce_network_event` 开头已有 `close_requested` guard，暂停
  状态不越过 teardown。
- **状态字段侵入**：优先推导（`recovery_attempt_started && 协商在飞 && !gate_open`），
  只有推导歧义时才加显式字段。
- **epoch 语义**：不 bump epoch 意味着暂停前 arm 的 deadline 仍然有效——正是 I-2；
  实施时跑 `reducer_tests` 全量确认无测试依赖旧行为（129d6fd7 后新增测试逐个过）。

---

## 提交与验收计划

| 顺序 | 提交 | 内容 | 前置 |
|---|---|---|---|
| 1 | `fix(hyper): publish transport retirement on graceful close` | 方案一（mp06） | 无，独立 |
| 2 | `fix(hyper): coalesce deferred restarts into the in-flight exchange` | 方案二（p08） | 建议在方案一合入后（减少同区噪音） |

流程沿用既有模式：fork 分支 → intra-fork PR → 合并更新 #542 head → CI Gate 验证。

**最终验收**：CI Gate `Test (Rust)` 全绿（当前 4 failed / 2 flaky → 0 failed；
`test_basic_ice_restart`、`test_mobile_half_open_15s` 两个 flaky 观察项——其失败
签名（软恢复升级）已由 `e1dd907b` 修复，理论上随之收敛）。

两个 PR 描述中分别引用 I-4..I-6 与 I-1..I-3 作为验收标准。
