<!-- language-allow-file: Chinese execution evidence -->

# 连接治理补测结果索引

## 最新 main 批次

| 字段 | 值 |
|---|---|
| 基线 | `1b7e2d89a75729a1bbaf8ff6711f8b9bea23f101` |
| 工作区 | `/Users/zhj/RustProject/Actrium/actrium-connection-governance-test-supplement` |
| 命令 | `cargo nextest run -p actr-hyper --features test-utils --test connection_governance_matrix --test connection_governance_production --test connection_governance_inflight --test-threads 2 --no-fail-fast` |
| 完整输出 | [all-final-main-1b7e2d89.out](all-final-main-1b7e2d89.out) |
| 结果 | 修复前完整批次：36 条用例，33 passed、3 failed、0 skipped（3 slow）；PR-0 绿色范围：32 passed、2 skipped |

### 已知失败

| 用例 | 分类 | 现象 |
|---|---|---|
| `answerer_failed_session_recovers_through_offerer_without_manual_reconnect` | CG-001 产品候选缺口 | Answerer 仅注入 Failed 后连续收到真实 restart Offer；四次 Request 都重新从 `attempt=1` 开始，未进入预期硬恢复换代 |
| `answerer_without_active_session_rejects_restart_before_new_connection` | CG-002 产品候选缺口 | Controller 仍存在但 active Session 已释放；新的 RestartOffer 未得到 `IceRestartRejected(NoActiveSession)` |
| `production_offerer_long_background_autonomously_rebuilds` | CG-003 契约/行为边界 | 真实信令重连成功，但无业务时清理后 20 秒内没有自动建立新会话；首个普通业务触发的对照用例通过 |

CG-001/CG-002 的复现用例保留在代码中，并在 PR-0 中显式跳过，分别由 #208/#209 的修复 PR 启用；没有通过放宽断言或修改生产代码来消除。CG-001 的 `Failed` 是测试注入的本端状态事实，不等于已经杀掉实际 PC 驱动。CG-003 的无业务主动重建断言已按 #210 的按需建连契约移除。

### 用例分组

| target | 数量 | 覆盖重点 |
|---|---:|---|
| `connection_governance_matrix` | 12 | 两角色、parked/open、Failed、共享锚点关闭、S4、60 秒自然耗尽、混合 peer、相同 ActorId 重启 |
| `connection_governance_production` | 20 | 真实 Node/Context/Mailbox/Workload/response_send，后台、超时、取消、容量、在途回复、两种流、Finish、重复恢复 |
| `connection_governance_inflight` | 4 | 两角色 × 入站/出站 × Call/Tell/DataChunk 部分提交，要求有界不确定且接收端不执行不完整消息 |

## 静态检查

| 检查 | 结果 | 证据 |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | 命令退出码 0 |
| targeted `cargo clippy ... -- -D warnings` | PASS | [clippy-main-1b7e2d89.out](clippy-main-1b7e2d89.out) |
| production build isolation (`cargo check -p actr-hyper --lib`, 及 `--features test-utils`) | BLOCKED | 本机 Cargo 缓存的 `webrtc-0.20.3` 源文件字段名破损（`udp_addspawn_address_monitorrs`/`udp_addrs` 不一致）；未修改第三方缓存 |
| CI gate 配置解析 | PASS | 见 worktree `.github/workflows/ci-gate.yml` 与 `.config/nextest.toml` |

## 辅助证据

- [notes.md](notes.md)：fixture、测试模型修正、设备可用性和失败归类。
- `all-final.out`：更新 main 前的完整批次，仅作历史对照，不能替代最新 main 输出。
- `matrix-final.out`、`production-final.out`、`inflight-first-run.out`：早期分批运行，仅用于定位，不替代完整批次。
- `no-active-session.out`：CG-002 的精简失败证据；完整 trace 在最新批次输出中。
- `same-id-restart-final.out`：相同 ActorId 重启两角色独立运行均通过；最终完整批次也包含这两条。
- `pr0-green.out`：按需建连契约修正后的 PR-0 测试批次，32 条通过、2 条显式跳过。

`*.log` 文件受仓库忽略规则影响，不作为唯一交付证据；可纳入版本控制的原始输出使用 `.out` 保存。

构建失败原始输出：[check-lib-main-1b7e2d89.out](check-lib-main-1b7e2d89.out)、[check-lib-test-utils-main-1b7e2d89.out](check-lib-test-utils-main-1b7e2d89.out)。
