<!-- language-allow-file: Chinese execution evidence -->

# 2026-09-08 补测执行记录

基线 `00138ee5`。新增生产 Node fixture 使用真实 Actrix Signaling 与生产 Node、Mailbox、Workload、response_send，仅注册凭证由 test fixture 提供。AIS 续期测试尚不能由这个 fixture 证明。

- 初次生产 fixture 用 discovery 作为注册就绪屏障失败：linked workload 没有 ServiceSpec，发现资格不是 transport 就绪条件。改为生产 SignalingClient 的 Ping/Pong barrier；归类为测试模型问题。
- 第一轮生产用例：6 PASS、1 FAIL，见 `production-first-run.out`。
- 长后台对照已核对：`production-expanded.out` 为13 PASS、1 FAIL；唯一失败是 Offerer 无业务自动重建。`production-long-offerer.out` 证明 Reconnect 成功、信令已恢复，但 mobile peer_count=0，remote Answerer 保留旧 generation=1 Recovering。增加首个普通业务触发的对照后，该对照通过。记录为“无业务自主恢复要求未满足／按需建连行为边界”，不能说所有长后台恢复都失败；生产代码没有修复。
- Android 无连接设备；`devicectl` 返回所有已配对 iOS 设备 unavailable。设备冻结/蜂窝切换暂不可执行。
- S4 无 active Session 用例失败：先实际关闭 Session 资源并递交 Closed 事实，确认 Controller 数量为1、pc_generation=0；真实 IceRestartOffer 到达，但未返回 IceRestartRejected。记录 `no-active-session.out`；与“Controller 不存在”的 Router 拒绝分支不同。尚未修复。
- 同ID重启旧断言要求首请求成功，实际得到带原operation_id的DeliveryUncertain。该错误符合提交后不确定契约，旧断言过强；修正为保留原操作终态、不重放，等待自然恢复/耗尽后再发独立新操作。不能把这个旧断言失败计作产品错误。

- 远端 main 后续更新到 `1b7e2d89a75729a1bbaf8ff6711f8b9bea23f101` 后重新执行完整 36 条：33 PASS、3 FAIL，仍是 CG-001、CG-002、CG-003，没有新增失败。输出为 `all-final-main-1b7e2d89.out`。
- targeted `cargo fmt --all -- --check`、`cargo clippy -p actr-hyper --features test-utils --test connection_governance_matrix --test connection_governance_production --test connection_governance_inflight -- -D warnings` 和 CI 配置脚本通过。普通 `cargo check -p actr-hyper --lib`（含 `--features test-utils`）被本机缓存的 webrtc-0.20.3 破损字段阻断，详见结果索引；没有修改第三方缓存。
- PR-0 按需建连契约移除两个长后台无业务主动重建用例，并显式跳过 #208/#209 复现用例；重新执行 32 条，32 PASS、2 SKIP，输出为 `pr0-green.out`。
