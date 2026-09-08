# 连接治理三个失败用例：根因与修复方案

## 1. 分析口径与结论

分析基线为失败批次对应的 `1b7e2d89a75729a1bbaf8ff6711f8b9bea23f101`。沿用该批次双端/Actrix 日志与源码，不把之后其他分支的修复视为本基线行为。随后已按 #210 调整无业务长后台测试契约；本文对 CG-001/CG-002 的生产修复仍是方案记录，未把测试跳过或契约调整当作生产修复。

证据：[完整批次日志](test-evidence/governance-20260908/all-final-main-1b7e2d89.out)、[结果索引](test-evidence/governance-20260908/results.md)、[覆盖报告](actr-connection-governance-test-supplement.main.zh.md)。日志时间以下均为 UTC。

| 用例 | 直接失败原因 | 定性 | 推荐修复 |
|---|---|---|---|
| CG-001：Answerer Failed 后恢复 | 同一未恢复周期内，重回 parked 清掉 request_id/attempt；对端反复 soft，无法收到 attempt≥4 | 恢复进度管理缺陷，源码与双端日志互相印证；注入 Failed 不证明实际驱动死亡 | 将 SDP 交换状态和逻辑恢复请求进度分开管理；超时保持并推进 Request，最终由 Offerer 换代 |
| CG-002：Controller 在、active Session 不在 | Router 只判断 Controller 存在；Reducer 将不满足 restart 条件的输入退回普通 Offer 分支，没有发送明确拒绝 | 协议分支缺口；后续普通 Offer 是否建成不能由当前失败断言证明 | 在 Controller 内判断 restart 资格并回复 NoActiveSession；Offerer 校验拒绝的 exchange 身份后硬恢复 |
| CG-003：清理后按需建连与 Request 反馈 | 全局清理移除了 Controller；新业务能重建，远端 Request 当前被 unknown-peer 分支丢弃 | 已确认不要求无业务主动重建；历史失败用例的这一断言超出契约 | 移除恢复计划方案；保留 RequestAck 协议反馈方案和后续新业务建连验收 |

CG-001/002 属于现有恢复路径中的实现问题。用户已明确 CG-003 不需要清理后无业务主动重建：连接可由后续新消息按需建立。此前从 Reconnect 名称/RFC 泛化出“必须主动恢复全部旧 peer”的建议范围过大，现撤回。原 36 条批次的执行记录保持不变，但 CG-003 的历史失败不再计作这一产品契约下的恢复缺陷；Request 没有明确反馈仍作为独立协议补齐项。

PR #204（`6ef9ab1e`）已包含在本基线，修复了生命周期/address parked 时健康旧 lane 的就绪事实丢失；相应用例通过。CG-001 是 Failed 撤销就绪后仍未恢复时的重传升级问题，保留 #204 的健康 lane 复用与故障 lane 撤销行为。

## 2. CG-001：Answerer 的恢复请求进度反复归零

### 2.1 实际前置状态

用例 `answerer_failed_session_recovers_through_offerer_without_manual_reconnect`，手机 A=Answerer(52101)，对端 B=Offerer(52102)。起始双方 PC generation=1、业务可用；仅 A 注入一次 Failed 事实，B 不注入事件、不执行手工重连。

`TestRtcControl::fail_peer` 通过真实事件 mailbox 注入状态，不关闭实际 PC。因而它验证的是“产品接收到 Failed 后怎么治理”，不是物理驱动退出复现。

### 2.2 双端时间线

| 时间 | A：Answerer | B：Offerer |
|---|---|---|
| 05:07:26.458 | Failed → rpc_reliable_open=false → parked，cycle=1、epoch=1 | 保持运行 |
| 05:07:32.461 | 第一个 6s deadline，发 Request：id=...1-0-2，attempt=1 | 收到后在 generation=1 做 ICE restart |
| 05:07:32.477 | 收到 restart Offer，回答完成并再次 Connected，但可靠锚点事实仍为 false，不能 Sendable | 同代次 soft 成功，恢复可发送 |
| 05:07:38.463 | 下一 deadline：重新 parked，epoch=3，清空 request 身份与计数；本次没有马上发 Request | 无新唤醒 |
| 05:07:44.465 | 再等 6s，发新 id=...1-1-4，attempt 又为 1 | 再做一次同代次 soft |
| 05:07:56.471 / 05:08:08.477 | 第 3/4 次仍是新 id、attempt=1，cycle 始终是 1 | 始终没有收到 attempt≥4 |

因此故障后 Request 大约在 +6s、+18s、+30s、+42s 发出，并非每个 6s deadline 都发。第 4 次 Request 后测试在“身份保持不变”的断言处失败；后面的普通 Offer、换代和业务恢复断言尚未执行。

### 2.3 根因链路

1. [reducer.rs](../crates/actr-hyper/src/wire/webrtc/v020/peer/reducer.rs) `reduce_state_tick` 的 Answerer Failed 分支撤销 `rpc_reliable_open`，进入 `wait_for_offerer_offer`。撤销故障代次的就绪权限本身应保留，不能用无条件置 true 修复。
2. 对端 restart Offer 使 `awaiting_offer=false`；Answerer 发送 Answer 后 `recovery_negotiated=true`。PC Connected 能到 `TransportReady`，但旧 lane 没有第二次 Open，锚点条件依然不成立。
3. `RecoveryDeadlineElapsed` 中“Answerer 重发请求”的普通分支要求 `!recovery_negotiated`，parked 分支要求 `awaiting_offer`。此时两个条件都不满足，落到 `start_hard_recovery`。
4. Answerer 的 hard 分支实际只调用 `wait_for_offerer_offer`。它调用 `clear_negotiation_for_recovery`，把 `local_restart_request_id=None`、`local_restart_request_attempt=0` 一起清掉。
5. 下一次计时才重新发送 Request，重新分配 id/attempt。B 的 `ApplyIceRestartRequest` 只有收到 attempt≥4 才硬恢复，进度永远达不到。

同一 A recovery_cycle 在日志中始终是 1，故这不是“完成恢复后合理开始新的请求身份”。B 每次 soft 成功后可以拥有自己的新周期，两端 cycle 数字不要求相同。

现有 `answerer_restart_request_retries_until_the_recovery_budget_without_rebuilding` 只模拟超时重传，没有把真实 restart Offer/Answer 完成再插回每一轮，因此未覆盖这个状态组合。

### 2.4 推荐修复

- 拆分“清理 SDP 交换”与“结束逻辑恢复周期”：前者清除过期 operation、SDP、candidate 和交换 id，保留同一逻辑周期的 Request id/attempt；后者在真正 Sendable、终态耗尽/Close、身份失效或明确开启新尝试时清理进度。
- 逐一审阅 `clear_negotiation_for_recovery` 的调用者，不应只删两行而把旧 request 身份带入所有新会话。普通新 Offer 建立替换尝试时应有明确的旧进度结束点；同一旧尝试的 parked↔协商转换不能结束它。
- 对“Answerer 已协商但仍不可发送”的 deadline，继续同一恢复请求进度。若要先清理协商并 parked，应在该 deadline 直接发送下一次 Request，避免无意义再空等一轮 6s。
- 保持 Offerer 为唯一普通新 Offer 发起者。收到单调递增 attempt，达到 4 后走已有 hard recovery；收到重复/迟到 attempt 不得创建额外 PC。
- 保持 A 的 60s recovery cycle 总期限；增加 attempt epoch 只能更新单次期限，不能更新总期限。重复 Failed/后台事件也不能无限重置期限。

本方案通过新的 PC 和真实 Open/Connected 恢复发送权限，不给失效 lane 伪造 OnOpen。将“Failed 后同一 PC 仍可 soft 恢复”作为另一个优化策略时，应增加明确的物理 lane 再验证和 generation/operation 防陈旧保护，不能夹带在本次最小修复中。

### 2.5 回归验收

新增 reducer/Controller 场景：每轮注入 restart Offer、完成 remote/local SDP、Connected，但保持 anchor 未就绪；下一 deadline 的 Request id 不变、attempt=1/2/3/4，A 不自己 BuildSession。再测试重复 Failed、gate 收紧/重开、丢 Request/Offer、迟到 completion，以及成功/Close/新尝试后身份确实清理。

保留当前真实双端回归，要求普通新 Offer、双方换代、双向 Call/Chunk 成功；原在途业务仍按自身 deadline/DeliveryUncertain 结束，不重放。S0 parked 且旧 lane 健康的用例必须继续同代次通过。

## 3. CG-002：没有 active Session 的 ICE restart 没有得到明确拒绝

### 3.1 前置状态和现象

手机 A=Answerer(52901)、B=Offerer(52902)。测试受控关闭 A 的实际 Session 资源并递交 Closed 事实，观察 `peer_count=1`、generation=0、sendable=false；保留逻辑 Controller。解除信令故障规则后，仅等待新一次 RestartOffer。

日志记录 B 在 05:08:21.476、05:08:27.478、05:08:33.481 发出 `rule=Deliver` 的 RestartOffer，exchange 分别为 `v020-1-1-10`、`v020-1-2-13`、`v020-1-3-16`；观察窗口内没有 IceRestartRejected。测试 30.433s 失败，其中包含 fixture 建立与 shutdown，拒绝等待窗口本身是 15s。

### 3.2 根因

[router.rs](../crates/actr-hyper/src/wire/webrtc/v020/signaling/router.rs) `route_inbound` 的 RestartOffer 分支只检查 `get_controller`：不存在则回复 NoActiveSession；存在则调用 `apply_remote_ice_restart_offer_correlated`。

Reducer 仅在“Answerer + active_generation 有值 + 非 building”时执行明确的 ICE restart 分支；否则回退 `ApplyRemoteDescriptionCorrelated`，即普通 Offer 处理。Controller 存在不是旧 PC/DTLS/SCTP 可复用的证明，不能让 restart SDP 隐式充当新连接 Offer。

当前日志证明无拒绝；源码证明落入错误的语义分支。当前测试不证明 fallback 一定成功创建了新 PC，也不证明后续数据已经丢失，应避免扩大结论。

### 3.3 推荐修复

1. 资格判断放在 Controller/reducer 的串行命令处理内，按当前 role、active Session 身份及 closing/building/pending 状态决定。仅在 Router 先读取 snapshot 再发送命令会有检查与执行之间的竞态。
2. active Session 缺失时，回显输入 exchange_id，发送 `IceRestartRejected(NoActiveSession)`；不把它改成普通 Offer，不隐式 BuildSession。Controller 不存在的现有拒绝路径保持一致。其他状态如替换中或旧交换迟到应单独规定拒绝/忽略语义，不能一律声称 NoActiveSession。
3. 拒绝是信令层响应，不能以“必须有活 PC”作为发送资格。可让 Controller 返回类型化处理结果，由 Router 用现有协议响应通道发送；同时携带/验证当前 signaling generation 和 cleanup 生命周期，避免旧 socket 的迟到响应跨清理边界发出。采用 effect 方案时也必须解决无 active generation 的派发和背压保留问题。
4. 同步修正 Offerer 处理拒绝的入口：当前 Router 对任意 IceRestartRejected 直接 `force_reconnect()`，没有比对 exchange_id。应转交 Controller 校验当前未完成的 **restart 类型** SDP exchange、PC/transport 身份，匹配时才 hard recovery；重复、已完成、旧代次拒绝只记 stale。
5. 比对不能只使用 `pending_local_restart_exchange_id`：该字段在发出 RestartOffer 时已被 `take()`，随后 id 保存到 `pending_local_sdp_exchange_id`。需要显式保留/识别 outstanding exchange 的类型，避免拒绝普通 Offer 或迟到拒绝误伤新的会话。

修复后的协议闭环应为：B RestartOffer → A NoActiveSession(exchange) → B 验证 exchange 后建新 PC/普通 Offer → A 按普通 Offer 建新 PC → 双方恢复业务。

### 3.4 回归验收

分别覆盖 Controller 不存在、Controller 存在且无 active、健康 active 可复用、active 被并发关闭、替换 pending/building、重复/过期拒绝、信令 gate/队列背压。

增强当前集成用例：验证 rejection 的 reason 和精确 exchange_id，并继续观察 B 仅启动一次有效 hard flight、A/B 新会话 Sendable、双向首 Call/Chunk 成功。另发旧 exchange 的拒绝，断言新代次不被重建。

## 4. CG-003：长后台清理后恢复意图没有交给新的 peer 生命周期

### 4.1 双端时间线

| 时间 | 手机 A=Offerer(62020) | B=Answerer(62010) |
|---|---|---|
| 05:10:04.578 | 前台事件携 65s 后台时长，Supervisor 执行 ForceReconnect；断信令、close_all | 观察到旧 RTC 失效，进入恢复等待 |
| 05:10:04.580 | 信令已连回；action completed success=true，residual_count=0 | 等待 Offerer 唤醒 |
| 05:10:09.305 / :15.307 / :21.311 | 三次日志 `dropping ICE restart request for unknown peer` | 每 6s 发恢复 Request |
| 05:10:24.581 | 观察窗口结束，peer_count=0 | peer_count=1、generation=1、Recovering、sendable=false |

对照用例在 A 清理后发首个普通新 Call，可自动建连且双向业务通过。因此网络、Actrix 服务以及普通首次建连入口均可工作。该测试只传入平台时长，未真的冻结进程 65s。

### 4.2 根因

- `translate_foreground` 对长后台派生 RequestReconnect，属于预期政策。
- [network_event.rs](../crates/actr-hyper/src/lifecycle/network_event.rs) `force_reconnect_typed` 调用 cleanup；cleanup 两次 `manager.close_all()` 排空 peer，并清理 PeerTransport。
- [manager.rs](../crates/actr-hyper/src/wire/webrtc/v020/manager.rs) `close_all` 终止旧 peer 生命周期/恢复义务，只保留 generation/revision 下限等防陈旧信息；这些不是待恢复 peer 意图。
- 后续只 `reopen_all_inbound_admission` 并恢复信令。重新开放入站只是允许新 Offer/RoleAssignment 创建 Controller，不会自己发 Offer。
- B 是 Answerer，只发 IceRestartRequest；Router 对未知 peer 的 Request 直接丢弃。A 没 Controller，gate 打开也没有接收者，两端互等。
- `execute_recovery_effect` 当前只为 **Restore** 捕获 peer 恢复目标；ForceReconnect 的 targets 为空，所以信令成功即可无 residual 地结束。把名称改成 Reconnect 并不自动满足 peer 恢复承接。

### 4.3 收敛后的方案：按需建连，补齐 Request 反馈

清理后允许 peer_count=0，不保存旧 peer 的主动恢复计划，也不在信令恢复后批量重新建连。后续新 Call 沿已有 PeerGate/Transport → 角色协商 → 新 PC 路径建立连接；本批 `production_offerer_long_background_first_business_dials_after_cleanup` 已验证该路径成功。

这里只能据已执行用例确认新 Call 的行为，不能直接外推为所有 Tell/Signal/流在所有发送策略下都会等待建连；非等待型载荷及 ExistingOnly/Blocked 仍遵守原准入规则。旧已提交请求不能当成新消息重放。

原 `production_offerer_long_background_autonomously_rebuilds` 要求“没有新业务也必须恢复”，应从必过验收中撤除，以按需建连用例承接；Answerer 侧恰好能被远端 Offer 唤醒的通过结果也不构成所有场景主动重建义务。本轮只更新方案和验收口径，尚未修改测试源文件或生成新的执行批次。

#### Request 的协议响应补齐

`crates/actr-protocol/proto/actr.proto` 已有 `IceRestartRequestAck`，携带 request_id、attempt，支持 ACCEPTED / NO_ACTIVE_SESSION / WRONG_ROLE；`signaling.proto` 也已有 relay 字段。当前 v0.20 `SignalingMessage` 与 `ws_transport` 转换未接入 ACK，Router 未处理。因此可以复用现有 wire 定义，不应拿只关联 SDP exchange 的 `IceRestartRejected` 来回复 Request。

推荐将 ACK 接入作为同一问题的协议补齐：

- 无 Controller / 无可接受请求的旧 Session：回复 NO_ACTIVE_SESSION，回显 request_id/attempt；是否可处理必须由实际资源所有者判断，构建中的替换会话应按其已承接恢复义务单独处理。
- 已由 Offerer 接受恢复义务：回复 ACCEPTED，它仅表示请求被接管，不代表 Sendable，也不能停止尚未收敛的恢复总预算。
- 当前稳定角色不能处理该请求：回复 WRONG_ROLE，不能自行交换 Offerer/Answerer。
- 请求方只接受属于当前未完成恢复请求的 ACK。NO_ACTIVE_SESSION/WRONG_ROLE 在本端恢复意图仍有效时触发一次有界的普通角色重新协商；由服务端重新确认角色，不允许旧 Answerer 直接造 Offer。明确的对端 Session 丢失与普通 Request 等待超时应为不同分支，保留 PR #195 的被动恢复约束。
- 重复、迟到、旧 Controller 生命周期的 ACK 不重复建连；新逻辑请求的 request_id 应在本地身份/Controller 生命周期内外具有足够唯一性，不能只依赖会复用的计数元组。
- 旧版本不发 ACK 时，仍允许合法 Offer/Answer 完成恢复，沿原有有界重传路径推进；ACCEPTED 丢失不影响 Offer 处理。新 ACK 的真实 Actrix relay、编解码和新旧端兼容性都需测试。

ACK 解决收到 Request 后如何明确告知对端旧 Session 已消失；它是针对收到的请求进行处理，不要求本机在没有业务、没有入站请求时主动建连。Manager 主动恢复计划已移出本次方案。

### 4.4 回归验收

- 手机 Offerer/Answerer 两角色：长后台清理完成、信令和发送准入恢复后，首个新 Call 无手工 connect 即可建立或使用新的连接，随后双向业务成功；无业务时允许保持未连接。
- 单端/双端同时长后台清理、信令恢复先后顺序、第一次角色协商/Offer 丢失；健康/已故障/仅入站建立的 peer 均纳入。
- 清理后新 Call、普通 Offer、Request/ACK 并发，只产生一次有效 Controller/PC flight；旧 generation 的 callback、Offer、Reject 不影响新连接。
- 与 Logout/Terminate/close_peer、身份重绑、连续 Foreground 竞态时遵守准入和身份边界；旧 pending 有界结束，保留原业务 deadline/投递不确定结果。
- RequestAck 的编解码、关联身份和旧端不回复兼容单独验证；多个 peer 不因某一条坏链路串行阻塞。

## 5. 实施顺序与验证边界

1. 先修 CG-001 的 Request 生命周期和已协商但未就绪的超时分支，限定在 peer 恢复进度，不更改业务重放策略。
2. 修 CG-002 的资格判断、类型化拒绝和拒绝关联校验，形成真正的双端换代闭环；它同时保护 CG-003 清理后的无旧会话场景。
3. CG-003 仅补 RequestAck 反馈及有界处理，按需新 Call 重建作为验收；不增加 Reconnect 目标恢复计划。

每步先补状态机/Controller 精确交错回归，再运行对应真实双端失败用例和健康对照；最后运行本批 36 条以及既有 mobile pair/multi-peer、信令拒绝/重试和生命周期 cleanup 测试。CG-001/002 通过后必须执行此前未到达的“新普通 Offer → 双端业务”断言，不能只看失败行消失。

本轮依据已有完整日志和源码定位，不把候选方案描述为已修复。此前普通构建曾被本机第三方 Cargo 缓存源码异常阻断；正式实施应使用来源可核验的依赖构建环境再验证，不能靠旧构建产物宣称修复有效。

## 6. 实施改动清单

以下接口名称为建议名称，不代表当前已经存在。

| 改动 | 位置 | 核心边界 | 完成标准 |
|---|---|---|---|
| 拆分 clear_sdp_exchange / finish_recovery_request | peer/reducer.rs，必要时 peer/state.rs | SDP 轮次清理不清 Request；真正成功、终态或新尝试才结束旧 Request | Failed 后多轮协商未就绪时，同 id、attempt 递增；healthy parked 不回退 |
| Answerer 未就绪 deadline 重传 | peer/reducer.rs / controller.rs | 包含 recovery_negotiated=true 但 !sendable；无 replacement build/pending 时重传，避免打断有效替换；gate 关闭不发信令 | 6s 推进一次，60s 总期限不被延长；Offerer 最终换代 |
| RestartOffer 资格判定及拒绝 | peer/reducer.rs、handle.rs、signaling/router.rs | 判断与状态串行；无 active 必须 NoActiveSession；协议响应可在无 PC 下发送且受信令生命周期保护 | 精确 reason/exchange_id，随后新普通 Offer 与双端业务通过 |
| Reject 身份校验 | peer/state.rs、reducer.rs、signaling/router.rs | outstanding SDP 显式区分普通 Offer/restart；只有匹配的未完成 restart 可升级 | 旧/重复 Reject 不改变新会话；当前 Reject 仅一次有效 hard flight |
| RequestAck 编解码与处理 | signaling/ws_transport.rs、router.rs、peer/reducer.rs / state.rs | 复用已有 proto；request_id/attempt 与生命周期匹配；ACK 不表示业务就绪 | Request 无 Session 明确反馈、重新协商有界；真实 relay 和旧端兼容通过 |
| 按需建连验收修正 | connection_governance_production.rs | 撤除无业务主动重建的必过要求；保留已通过的新 Call 建连对照并覆盖另一角色 | 清理后新操作可建连，旧不确定操作不重放 |

可分别实施 CG-001、CG-002 和 CG-003 的 RequestAck 补齐；CG-003 不再包含无业务主动恢复。验证业务包括双向 RPC、Tell、DataChunk，并按各载荷的准入契约断言；原已提交操作保持 DeliveryUncertain/原 deadline，不因连接修复自动重放。
