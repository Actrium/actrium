# actr 连接治理补充测试与问题记录

## 1. 范围和基线

本轮补测围绕“手机是 Offerer / Answerer × 旧连接可复用 / 需要替换 × 恢复中发送与接收”展开。新增 3 个集成测试 target、34 条用例；参数循环另外展开角色、方向、载荷和恢复轮次，不能把这些次数当作独立测试数或覆盖率。修复前的最新 main 完整批次为 **33 passed、3 failed、0 skipped**；按需建连契约调整后的 PR-0 批次为 **32 passed、2 skipped**，两个跳过项分别等待 #208/#209 修复。

| 项目 | 当前值 |
|---|---|
| 工作分支 | `codex/connection-governance-test-supplement` |
| 最新验证基线 | `1b7e2d89a75729a1bbaf8ff6711f8b9bea23f101`，2026-09-08 查询并 fast-forward 到远端 main |
| 初始基线 | `00138ee5f59a6b0a568d73185eb946a2fa286bb1`；旧批次日志保留此身份 |
| main 增量 | CI/协议标识审计及 v0.10.0 发布版本更新；RTC 治理实现没有变化，仍重新构建和验证 |
| 行为变更 | 未修复生产连接、恢复或发送逻辑；`src/test_support` 只增加 test-utils 夹具和观测入口 |
| CI 接入 | 3 个 target 加入 realtime 分片，nextest 对本批测试 `retries = 0`；失败用例保持启用 |
| 提交和推送 | 未执行 |
| 排除范围 | 按用户要求，不补“RTC/WS 双路回退与投递状态”专项 |

**不能宣称连接治理已经全面覆盖或测试全绿。** 最终执行结果和逐项名称见[结果索引](test-evidence/governance-20260908/results.md)。下文分别记录覆盖、失败和剩余缺口。

## 2. 测试装配及证据边界

| 文件 | 数量 | 实际经过的链路 |
|---|---:|---|
| [connection_governance_matrix.rs](../crates/actr-hyper/tests/connection_governance_matrix.rs) | 12 | 真实 Actrix Signaling、真实 RTC PC/DataChannel、生产 Controller/Transport；业务端是 TestHarness responder |
| [connection_governance_production.rs](../crates/actr-hyper/tests/connection_governance_production.rs) | 18 | 真实 Node → Context → PeerGate → Transport → 对端 Mailbox/Workload → 生产 response_send；真实流注册、回调和注销 |
| [connection_governance_inflight.rs](../crates/actr-hyper/tests/connection_governance_inflight.rs) | 4 | 与生产装配相同；手机两角色 × 两方向，每条各检查 Call、Tell、DataChunk，共 12 个部分提交组合 |
| [support/governance.rs](../crates/actr-hyper/tests/support/governance.rs) | 共享夹具 | 双端独立 Node/存储、真实信令注册就绪屏障、可暂停的真实 Handler/流回调、接收日志 |

生产夹具仅以 fixture RegisterOk 提供初始 AIS 签发结果，使用生产 startup 和 SignalingClient Ping/Pong 等待注册完成。**没有运行 AIS 签发/续期服务，因此不能用它证明过期续期、RealmDenied 或重新签发的完整链路。**

故障模型必须区分：

- `fail_peer` 只注入本地 Failed 事实，实际 PC 仍可能存活；不等于杀死驱动或物理网络断开。
- `close_lane(RpcReliable)` 真正关闭可靠锚点。对端也能观察到共享通道关闭，可以自主发起硬恢复。
- S4 使用测试控制口实际释放 Session，再递交 Closed 事实，观察 Controller 仍存在、active generation 为 0；属于受控资源故障。
- Background/Foreground、Unavailable/Available 是生产网络事件处理器收到的归一化平台事件。`foreground(..., 65_000)` 提供后台时长，进程并没有被操作系统冻结 65 秒。
- 60 秒自然耗尽用例等待真实墙钟，不加速时钟；多 peer 用例使用精确到两端 generation 的包黑洞，只故障一条链路。
- 没有执行 Android Doze、iOS suspend、真实蜂窝切换或系统级断网。

## 3. 角色 × 旧连接状态

| 场景 | 手机 Offerer | 手机 Answerer | 双端关键断言 |
|---|---|---|---|
| 短后台后旧通道仍可用 | 生产用例覆盖 | 生产用例覆盖 | 同一个 DataChannel Arc、双方 PC generation 不变，首个双向 RPC/Chunk 和 Finish 成功 |
| Answerer 逻辑 parked，但旧 lane 仍 Open | 对端承担 restart Offer | 单独覆盖 local_address_changed 触发 awaiting_offer 的链路 | 真实 Request/RestartOffer 后复用同一通道，无需第二次 OnOpen，lane 数不增加 |
| 本端收到 Failed、对端仍运行 | 同代次 soft recovery 用例 | 暴露 CG-001 | 事实注入不能外推为真实 Driver 退出 |
| 旧 RpcReliable 锚点真实关闭 | 自动普通 Offer / 换代 | 对端 Offerer 可直接换代 | 两端 generation 增长；已有提交按 DeliveryUncertain 收束，pending 清零；后续双向业务成功 |
| Controller 存在、active Session 不存在 | 作为发送 restart 的对端 | 暴露 CG-002 | 必须先观察 S4，再观察解除故障规则后的 RestartOffer |
| 长后台全局清理、恢复后无新业务 | 按需建连契约 | 无业务时允许保持未连接；Offerer 有首个普通业务触发的成功对照 | 新业务不调用手动 connect/force 即可建连 |
| 60 秒恢复预算自然耗尽 | 覆盖 | 覆盖 | RecoveryExhausted、active generation=0、不可发送、pending=0、旧 DestTransport 被清退 |
| 远端按相同 ActorId 重启 | 覆盖 | 覆盖 | 原请求有界结束；不重放不确定操作，恢复/耗尽后独立新请求成功 |

## 4. 恢复期间消息覆盖

| 切点 | 本轮断言 | 边界 |
|---|---|---|
| 准入前等待 | Background 中 Call 保持等待且对端未执行；Foreground 后只执行一次 | 真实 Context，不用手写成功返回替代 |
| 等待超时 | 100ms deadline 得到 TimedOut；恢复后旧请求仍未执行，新 Context 可发送 | 不延长原请求 deadline |
| 等待取消 | abort 并 join 发送任务；恢复后没有旧请求执行，新操作成功 | 发送任务真正结束 |
| 容量上限 | 同 peer 65 个等待请求，1 个及时 Unavailable(waiter capacity)，64 个恢复后各完成；后续调用正常 | 尚未覆盖全局等待字节上限 |
| 部分提交 | 200KiB Call/Tell/Chunk 的第一个真实 fragment 后暂停；实际关闭锚点、等待新代次，再放行 | 两角色 × 两方向 × 三载荷均要求 DeliveryUncertain；接收侧完整 Handler/Chunk 执行数为 0 |
| 已进入 Handler、回复尚未发送 | 真实 Handler 持有回复，短后台恢复后两种手机角色均回包一次 | 服务端 → 手机方向 |
| 已提交后本端 Failed | 调用方自己的 Answerer 失败，在业务 20s deadline 前按恢复宽限结束 | 精确要求 DeliveryUncertain |
| 已提交后仅远端 Failed | 服务端仍健康时，原调用按业务 deadline 或可见的投递失败结束，执行计数为 1 | 不能要求远端瞬时获知手机本地故障 |
| 新代次后发送 | 双向 Call/Tell，RpcReliable/RpcSignal；StreamReliable/StreamLatencyFirst 各 8 × 32KiB，真实接收 payload 和次数 | 不把 Tell 无 pending 误当作已执行 |
| Finish 与回调 | 已进入的 callback 暂停，Finish 返回、其他 RPC 不阻塞；放行后该 callback 完成一次 | 不是“仍在网络中的尾块被 Finish 超车”测试 |
| 多 peer 隔离 | 手机同时对两个 peer 分别是 Offerer/Answerer；仅一链路黑洞，健康链路双向 RPC/Chunk 成功且不换代 | 使用 transport harness，不是三套生产 Workload |
| 重复恢复资源 | 两角色各 6 轮 soft/hard 交替；每轮双向 RPC/流/Finish，peer_count=1、live_lanes≤2、无 inbound drop | 有限循环，不能证明 30 分钟 FD/RSS/任务数稳定 |

## 5. 暴露的问题和候选方向

逐条双端时间线、源码因果链、推荐改动与回归验收见[三个失败用例的根因与修复方案](actr-connection-governance-failure-analysis.main.zh.md)。

### CG-001：Answerer 持续未恢复时，restart 请求进度重复归零

用例：`answerer_failed_session_recovers_through_offerer_without_manual_reconnect`。

只给 Answerer 注入 Failed。真实对端收到并处理 Request，反复返回 ICE restart Offer；本端旧可靠锚点的就绪事实被撤销，同代次协商结束后仍不能 Sendable。约 42 秒收齐四轮 Request 后断言失败：

| 次数 | request_id | attempt |
|---:|---|---:|
| 1 | v020-request-1-0-2 | 1 |
| 2 | v020-request-1-1-4 | 1 |
| 3 | v020-request-1-2-6 | 1 |
| 4 | v020-request-1-3-8 | 1 |

本次观察窗口内，连续未恢复的尝试没有推进到 Offerer 的 attempt≥4 硬恢复阈值。代码候选链路为 `wait_for_offerer_offer → clear_negotiation_for_recovery` 清理协商时同时重置本地 restart identity。**不能据此声称“每次成功协商后也必须永久保留同一 request_id”，也不能把 Failed 注入称为物理 PC 已死亡。**

可能方向：区分单轮 SDP 交换身份和持续失败的恢复进度；在仍未 Sendable 时保留/累计升级依据，确认硬恢复阈值能被对端真正观察到。需要协议确认及产品修复，本轮仅记录。

### CG-002：Controller 存在但无 active Session 时，没有明确拒绝 ICE restart

用例：`answerer_without_active_session_rejects_restart_before_new_connection`。

先真实释放 Answerer Session，保持 Controller 数量为 1，并观察 generation=0、sendable=false。解除 RestartOffer 的 drop 规则后，只把新观测到的 offer 算作刺激；等待 15 秒没有收到 IceRestartRejected。

[router.rs](../crates/actr-hyper/src/wire/webrtc/v020/signaling/router.rs) 在 Controller 不存在时发送 NoActiveSession；Controller 存在时把请求交给 [reducer.rs](../crates/actr-hyper/src/wire/webrtc/v020/peer/reducer.rs)，后者无 active 的 fallback 按普通 Offer 处理。两条分支的旧 Session 协议语义不一致，不能用“unknown Controller 拒绝测试通过”替代此场景。

可能方向：在执行 restart 前按实际 active Session 资格判断，并关联 exchange_id 返回 NoActiveSession；对端据此发布普通新 Offer，验证两端换代及新业务。当前用例在拒绝断言处失败，因此后续收敛尚未被证明。

### CG-003：长后台清理后的 Offerer，无业务自动重建要求未满足

用例：`production_offerer_long_background_autonomously_rebuilds`。

真实生产 Node 处理后台及 65 秒后台时长的前台事件。信令重连成功，但 20 秒观察窗口内，手机 peer_count=0；远端 Answerer 仍保留旧 generation=1、Recovering。没有观察到双方自主建立新可发送会话。

对照 `production_offerer_long_background_first_business_dials_after_cleanup` 通过：明确观察清理后，发送一个普通新 Call 即能建连，随后双向业务正常。Answerer 的无业务恢复用例也通过。

用户后续已明确不需要无业务主动重建，采用后续新业务按需建连。因此这一历史失败归为验收要求过强，不再计作该契约下的产品恢复缺陷；首个新 Call 建连的对照已经通过。方案撤除主动恢复计划，仅保留 RequestAck 协议反馈补齐。PR-0 已删除这条无业务主动重建断言；历史 36 条的原始结果不改写。

## 6. 测试模型修正

1. 直接关闭共享 DataChannel 时，对端 Offerer 也会发现关闭并硬恢复，不能强制要求 Answerer 必须先发送 Request；已拆开共享关闭与单端 Failed。
2. 早期 parked 测试仅触发短后台，证据不足；已改为 local_address_changed 并检查物理通道身份、真实 restart 信令及同代次恢复。
3. 生产 fixture 最初用 discovery 当注册就绪条件，因 Workload 未声明 ServiceSpec 而失败；改用生产信令 Ping/Pong 屏障。没有计作产品缺陷。
4. 远端单侧 Failed 与调用方本地 Failed 的可见时间不同；前者保留业务 deadline，后者检查本地 pending 宽限。
5. 同 ActorId 重启时，首请求可能进入旧 wire，DeliveryUncertain 是合法终态。原先“首请求必须成功”的断言过强，现验证有界终态、不自动重放，以及独立新操作成功。旧批次的这两项失败不计入产品问题数。
6. S4 观测必须排除此前被故障规则丢弃的 RestartOffer；以解除规则前 hits 为基线等待新 offer。

历史试跑、最终结果分开保存；以结果索引指定的最终文件为准，不用中断的输出或手工摘要替代完整运行。

## 7. 对原 G01–G15 清单逐项回填

编号沿用删除“RTC/WS 回退”专项后的清单；当前 G09 指 Tell/Signal/媒体。

| 项目 | 已补内容 | 尚未证明 / 后续验收 |
|---|---|---|
| P0 G01 | 真 RTC Answerer parked 旧 Open lane；生产短后台双角色同通道、双向首业务 | 真机挂起后相同状态的复核 |
| P0 G02 | 双角色共享锚点真实关闭、自动换代、旧提交不确定、新业务 | 单端物理损坏且对端不知情不能由共享关闭替代；CG-001 暴露相关进度问题 |
| P0 G03 | Failed 事实与 S4 分开；CG-001 / CG-002 保持失败 | 独立 Driver 退出的真实 supervision 路径；Handler API 本身不发 DriverExited，不能伪称有实测 |
| P0 G04 | Offerer 旧连接复用、Failed soft、锚点关闭 hard、同 ID 重启 | 精确的物理驱动失效及真机网络变化 |
| P0 G05 | 两角色生产长后台、服务端在途回复、双向部分提交 | CG-003 未满足无业务要求；真实进程冻结仍缺 |
| P0 G06 | 生产 Context/Mailbox/Handler/response_send、等待/取消/提交切点 | AIS 真实签发没有经过；没有枚举所有调度交错 |
| P0 G07 | 新代次首 Chunk、两种流、已进入 callback 的 Finish drain | 未接收尾块被 Finish 超车、延迟 lane 注册、同 stream_id 重注册与残留数据 |
| P0 G08 | 已检查设备可用性 | adb 无设备；已配对 iOS 设备均 unavailable，Doze/锁屏/suspend/进程杀死未执行 |
| P1 G09 | 真实双向 Tell/RpcSignal/两种 DataStream 的 Handler/Registry 计数 | RTP 媒体 track 跨代次替换未补，不能套用 DataChannel 结果 |
| P1 G10 | 两角色真实 60s 耗尽、pending 宽限、业务超时和取消、新 Context | 5s/6s/60s 与 SDP/回复同刻前后所有边界排列尚未穷举 |
| P1 G11 | 复核已有 credential_renewal_visibility / turn_credential_rotation 边界 | AIS 过期/暂时失败/RealmDenied × 挂起 × RTC × TURN 联动未补 |
| P1 G12 | 相同 ActorId 重启，两角色原操作终态与独立新操作 | 新 ActorId 的发现结果更新、业务重新绑定精确目标未补 |
| P1 G13 | 三 peer 混合角色坏链隔离，65 个 per-peer waiter 上限 | 全局等待字节、大流竞争和不同 peer 的容量公平性 |
| P1 G14 | 单链路受控黑洞、归一化网络不可用事件 | 真蜂窝双向切换、无 Offline 回调、VPN、无互联网 Wi-Fi、IPv6/NAT64、TURN-only 选路矩阵 |
| P2 G15 | 两角色各 6 轮恢复与 lane 资源回收 | 30 分钟以上 FD/RSS/任务数曲线；历史 ignore 独特断言的迁移尚未完成 |

剩余项是未完成的覆盖工作，不能全部归因于设备缺失：G07/G09/G11/G12/G13/G15 中仍有可用新夹具继续补充的软件测试。真机 G08/G14 则需要设备及可切换网络的实验环境。

## 8. 复现和交付

在补测 worktree 根目录执行：

```sh
cargo fmt --all -- --check
cargo clippy -p actr-hyper --features test-utils --test connection_governance_matrix --test connection_governance_production --test connection_governance_inflight -- -D warnings
cargo nextest run -p actr-hyper --features test-utils --test connection_governance_matrix --test connection_governance_production --test connection_governance_inflight --test-threads 2 --no-fail-fast
```

生产构建隔离验证已尝试 `cargo check -p actr-hyper --lib`（及 `--features test-utils`），但本机 Cargo 缓存中的 `webrtc-0.20.3/src/peer_connection/mod.rs` 将字段声明写成了 `udp_addspawn_address_monitorrs`，而其他位置仍引用 `udp_addrs`，导致第三方依赖编译失败；没有修改该缓存。目标测试的 targeted clippy 和 nextest 编译/执行均通过其自身构建路径。本轮不提交 PR，所以未宣称已经执行提交 PR 所要求的全 workspace clippy --fix 检查。

证据入口：[最终结果及批次索引](test-evidence/governance-20260908/results.md)、[历史执行/模型修正记录](test-evidence/governance-20260908/notes.md)。原始输出以可纳入版本控制的 `.out` 保存；旧 `.log` 被仓库忽略，不作为唯一可交付证据。
