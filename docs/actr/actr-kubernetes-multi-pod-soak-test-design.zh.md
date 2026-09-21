# actr 浸泡测试方案

- 状态：Proposed，作为实施设计和发布门禁基线
- 代码基线：main 分支；每次运行由 CI 固定并记录实际 checkout 的 commit SHA
- RTC 基线：main 当前启用的 WebRTC v0.20 后端
- 运行环境：GitHub Actions、自建 Kubernetes 集群、无真机
- 适用对象：actr RTC、信令、恢复、业务传输和长期资源稳定性

## 测试 Workflow

```text
历史 Issue / 测试问题
        ↓
整理成事件库和固定 seed 的测试计划
        ↓
GitHub Actions 部署 Kubernetes 测试环境
        ↓
单 Actr 预检 / 多 Actr Endpoint 通过 Actrix 建立双向通信
        ↓
Dashboard 启动长时间浸泡测试
        ↓
持续发送业务并注入事件
        ├─ 移动/网络状态 → NetworkEventHandle
        ├─ RTC 网络故障 → WebRTC v0.20 Runtime/UDP Socket 层
        └─ 信令故障     → 信令传输层
        ↓
真实 Controller / Session / DataChannel 链路执行
        ↓
统一收集日志、状态和指标
        ↓
结果判定器 + AI 判断故障
        ↓
生成 Issue 草稿、复现 seed 和分析结果
```

---

## 术语表与命名约定

测试侧进程的推荐名称是 **Actr Soak Endpoint（Actr 浸泡端点）**，推荐的
二进制、镜像和 Kubernetes 应用名称为 `actr-soak-endpoint`。这个名称直接
表达“运行浸泡测试的 Actr 端点”，避免 “Agent” 与 AI Agent 或 Actrix 服务
混淆。当前实现中仍有少量二进制、镜像、selector 和指标使用
`actr-soak-agent`；这些只是改名期间的兼容别名，不是拓扑身份。

独立测试端点的稳定身份仍统一命名为 `actr-0`、`actr-1`、……、`actr-N`。
每个名称对应一个 Kubernetes Pod 和一个 Actr RTC 进程。从一个端点的角度
看，另一个端点称为 Peer，例如 `actr-1` 是 `actr-0` 的 Peer。
`peer-0`、`peer-1` 可以作为界面显示别名，但不是稳定的 Actr 身份，拓扑和
credential 配置必须使用 `actr-N` 名称。

| 术语 | 本方案中的含义 |
| --- | --- |
| **Actr Soak Endpoint** | 一个测试 Pod/进程，运行真实 Actr RTC 端点和测试专用控制适配器；推荐实现名称为 `actr-soak-endpoint`。 |
| **Endpoint identity** | 拓扑中的稳定名称 `actr-N`，与 ActrId、Pod IP、Pod UID 分开。 |
| **Peer** | 某条连接边上的远端端点；关系从观察端点的视角是有方向的。 |
| **Endpoint pool** | CI 部署的固定大小 Endpoint 集合，启动时不带预设 Peer 图；池成员以租约方式分配给各个 run。 |
| **Peer wiring** | SoakController 在运行时通过 peer 管理 API 建立和拆除 Peer 边；接线的图记录在 `plan.json` 中。 |
| **Lease（租约）** | 把池的一个子集独占分配给一个 run；不相交的租约允许并发运行。 |
| **ActrNode** | 运行在 Actr Soak Endpoint 内的生产 Actr 节点。 |
| **Actrix** | Endpoint 使用的真实信令/STUN/TURN 服务，不是 Endpoint，也不是测试控制器。 |
| **Soak Dashboard** | 提供 UI/API 和持久化运行记录，并内嵌 SoakController。 |
| **SoakController** | 根据 seed 生成并执行动作、调用 Endpoint、收集证据并运行结果判定器的编排器。 |
| **Endpoint Control API** | 每个 Endpoint 在测试 namespace 暴露的 HTTP 控制适配层，只接受预定义且经过认证的动作和观测请求。 |
| **Agent API / `actr-soak-agent`** | Endpoint Control API 和端点二进制的旧实现名称，仅用于迁移兼容。 |
| **Seed** | 用于可重复生成事件计划和动作顺序的随机种子。 |
| **Event plan** | 由 seed 生成的带时间戳动作清单，包括业务探针和注入事件。 |
| **Lifecycle event** | 通过 `NetworkEventHandle` 送入生产链路的 Background、Foreground、Available、Unavailable 等逻辑移动事实，不是真机操作。 |
| **Fault** | 作用于 RTC 或信令 seam 的测试专用、限定范围的 drop、delay、blackhole 等故障规则。 |
| **Business probe** | 经过正式 Actr 业务链路发送的 RPC 或 DataChunk 请求。 |
| **Snapshot / Hook** | 提供给测试观测的只读状态快照和状态转换回调。 |
| **结果判定器（Result Evaluator）** | 对业务正确性、恢复、fencing、隔离和资源收敛进行确定性判定的规则；当前实现目录仍为 `oracle/`。 |
| **Artifact** | 不可变的运行证据，包括计划、时间线、日志、快照、指标、结果判定器输出和重放脚本。 |
| **Preflight** | 长时间浸泡前的短时部署和跨 Pod 就绪验证，检查镜像、凭证、Actrix 连通性和一次业务交换。 |
| **Resource convergence** | 反复恢复后 pending、session、lane、FD、socket、RSS、CPU 等回到稳定范围，而不是无界增长。 |

本次先在文档中完成命名迁移，不会静默改名已部署的二进制或 selector。后续
实现 PR 可以把 `actr-soak-endpoint` 设为主 target，并短期保留
`actr-soak-agent` 别名；`actr_soak_agent_*` 指标也应等版本化迁移方案确定后
再改名。

## CI 与 Dashboard 职责

当前采用“CI 部署、Dashboard 控制测试”的模式：

| 组件 | 负责内容 | 不负责内容 |
| --- | --- | --- |
| GitHub Actions / CI | 固定 SHA、构建并推送 Actrix/Endpoint/Dashboard 镜像；部署 namespace、Secret、RBAC、NetworkPolicy、Actrix、未接线的 Endpoint 池（`actr-0...actr-N`）和 Dashboard；等待 Pod 与 Dashboard 健康 | 不创建测试 Controller Job，不生成测试计划，不接线 Peer 边，不注入 Fault，不启动浸泡业务循环 |
| soak-dashboard | 提供 UI/API；预览和启动测试计划；租用池子集并通过 peer 管理 API 接线/拆除 Peer 图；调用 Endpoint 控制 API；注入生命周期、网络、RTC/信令事件；收集指标、日志、结果判定器输出和 artifact | 不构建镜像，不负责 CI 发布，不应获得 cluster-admin |
| Endpoint (`actr-N`) | 运行真实 Actr RTC 端点和测试 workload；通过 Actrix 建立 signaling/ICE/DTLS/SCTP/DataChannel；提供控制和观测接口 | 不直接修改 Controller 内部状态，不绕过 PeerGate 操作 DataChannel |

Dashboard 内嵌 Soak Controller，因此测试不需要 CI Job。CI 只负责部署，不能启动或
控制浸泡测试。Nightly/Weekend 由操作员或 Dashboard 调度器提交到 Dashboard 队列，
运行期间独占整个池；并发与租约规则见下节。

Dashboard 当前运行状态写入 PVC；Dashboard Pod 重启会将正在运行的任务标记为
Interrupted，不会自动续跑。因此 Dashboard 适合人工调试和受控长跑，正式无人值守
长跑仍需外部调度和故障恢复策略。

### 拓扑归属与计划兼容性

CI 拥有的是 Endpoint 池，而不是拓扑。一次部署只固定池大小、规范身份
（`actr-0` 到 `actr-N`）、Actr credential 和镜像。Endpoint 启动时不带预置
Peer 图；`SOAK_REMOTE_PEERS`、`SOAK_INITIATOR_PEERS` 这类启动期接线不再是
部署契约的一部分。

Dashboard 把拓扑作为计划数据来拥有。每个计划在 `plan.json` 中携带显式的 Peer 图；
run 启动时，内嵌 SoakController 通过 Endpoint 控制 API 的 peer 管理接口完成接线
并验证连通性，run 结束时拆除。改变拓扑形状（Pair 到 Multi-peer、ring 到
hub+leaf）是修改计划，不是重新部署。

计划兼容性简化为池适配：计划的 Endpoint 数量不超过空闲池大小、形状被
SoakController 支持即为合法。Dashboard 必须在启动前拒绝放不进空闲池的计划。

Dashboard 为每个 run 分配池的独占子集（租约）。事件/Fault 覆盖类 profile
（pair-event、process-churn、multi-peer）允许在不相交的子集上并发；资源敏感
profile（Nightly、Weekend、release）必须独占整个池。放不下的 run 在 Dashboard
队列中等待；23:00 定时调度器也向该队列提交 run。这取代了早期"同一时刻只允许
一个 run"的硬限制。

池复用卫生纪律是契约的一部分：

- run 必须通过收尾 quiescence 检查（pending、Fault lease、recovery obligation
  全部清零），其租约子集才能归还空闲池；
- 每个 run 开始时对租到的 Endpoint 执行 `/v1/actions/cleanup`，并重新采集资源基线；
- 每个 run 在共享 Actrix 上使用独立 realm/ActrId 前缀，结束前必须确认本 run 的
  TURN allocation 和注册已排空；
- 池按计划定期滚动重建，release 门禁 run 前也必须重建。

调整池大小仍需 CI 重新部署。Dashboard 有活动 run 时，CI 不得重新部署同一
namespace。第一版采用"一套 namespace 对应一个 Dashboard"；以后如果要做跨
namespace Dashboard，需要单独设计，不能削弱这个控制边界。

---

## 1. 一句话和完整流程

一句话：

> CI 在 Kubernetes 中部署真实 Actrix、未接线的 Endpoint 池（`actr-0` 到 `actr-N`）和 `soak-dashboard`；Dashboard 再按计划接线 Peer 图、用固定 seed 启动浸泡测试，持续发送双向业务、随机注入移动事件和 RTC/信令 Fault，并根据端点状态、业务结果、Actrix 指标和 Kubernetes 资源指标判断恢复正确性与长期稳定性。

完整流程：

~~~text
GitHub Actions / ARC Runner
  → 固定 main SHA，构建并推送 Actrix、Endpoint 和 Dashboard 镜像
  → 创建/更新 namespace、Secret、RBAC 和 NetworkPolicy
  → 部署 Actrix、未接线的 Endpoint 池（actr-0 到 actr-N）、Dashboard 和采集器
  → 只等待 Pod/Dashboard 健康，不启动浸泡计划

soak-dashboard（操作员/API）
  → 预览或启动测试，根据固定 seed 生成事件计划
  → 租用池子集，通过 peer 管理 API 接线计划中的 Peer 图并验证连通性
  → 内嵌 SoakController 执行端点 preflight 和双向业务
  → Endpoint 通过真实 Actrix 完成 signaling、ICE、DTLS、SCTP、DataChannel
  → 持续发送 RPC、DataChunk 和分片业务
  → 通过 Endpoint 控制 API 注入生命周期、网络、RTC、信令事件
  → 由操作员或明确启用的生命周期控制器执行 SIGTERM、SIGKILL、Pod 重建和 Actrix 重启
  → 收集 Snapshot、Hook、Fault 命中、业务结果、Actrix 和 K8s 指标
  → 结果判定器判断业务、恢复、隔离、generation fencing 和资源收敛
  → 将完整 artifact 和 reproduce.sh 写入 Dashboard PVC
  → 通过 quiescence 闸门后拆除接线，把租约子集归还池中
  → 操作员或独立只读分析器可在运行结束后导出 artifact 并调用 Claude Code
  → 人工修复后使用原 seed 重放
~~~

运行时间：

- Nightly：北京时间 23:00 至次日 07:00，连续 8 小时；
- Weekend：周五 23:00 开始，连续 48 小时；
- 发布前：根据资源容量增加 8～24 小时，禁止自动重试。

---

## 2. 测试目标与边界

### 2.1 目标

- 验证多个 Kubernetes Pod 中的 actr Endpoint 能通过真实 Actrix 完成双向 RTC 业务；
- 验证后台、前台、逻辑断网、网络恢复和网络类型事件能驱动生产 recovery；
- 验证 RTC UDP Fault、信令 Fault、在途请求和 DataChannel 关闭后的 Soft/Hard Recovery；
- 验证同一 Peer 故障不会阻塞其他 Peer 的 signaling、业务和 Controller；
- 验证 pc generation、transport epoch、revision、role、exchange 和 stale fencing；
- 验证进程重启、Pod 重建、同身份重启、新 ActrId 重启和 Actrix 重启；
- 验证长时间运行后 pending、session、lane、FD、socket、RSS 和 CPU 不持续增长；
- 形成可重放、可审计的测试闭环；CI 负责构建部署，Dashboard 负责启动和控制测试。

### 2.2 非目标

- 不声称替代真实 Android/iOS 的进程冻结、系统网络切换和无线基带行为；
- 不把测试 Fault Registry 当成 Linux 内核、CNI、NAT、MTU 或运营商网络的完整仿真；
- 不在测试过程中修改正式 SDK 的业务语义；
- 不让 AI 修改 PASS/FAIL、放宽阈值、自动重试或直接修复生产代码；
- 不把 Kubernetes liveness 当成 RTC recovery 成功条件。

### 2.3 四条不变原则

1. RPC、DataChunk 和分片业务必须经过真实 actr RTC 数据面；
2. 生命周期事件通过生产的 NetworkEventHandle 进入 recovery；
3. Fault 只能通过明确的测试构建和测试控制面启用；
4. PASS/FAIL 由确定性的结果判定器决定，AI 只输出辅助分析。

---

## 3. 总体架构

### 3.1 简化拓扑

~~~text
GitHub Actions
      │
      ▼
  只构建/部署，不启动测试
      │
      ▼
┌──────────────────┐       Endpoint 控制 API       ┌──────────────┐
│ soak-dashboard   │ ─────────────────────────────▶ │ actr-0       │
│ UI + BFF +       │                                │ Actr + ctl   │
│ 内嵌             │                                └──────┬───────┘
│ SoakController   │                                       │ 真实 RTC
└──────────────────┘                                       │
      │                                                    │
      └────────────── Endpoint 控制 API ───────────────────▶┌──────▼───────┐
                                                           │ actr-1 ... N │
                                                           │ Actr + ctl   │
                                                           └──────┬───────┘
                                                                  │
                                                                  ▼
                                                           ┌─────────────┐
                                                           │ Actrix      │
                                                           │ WS/STUN/TURN│
                                                           └─────────────┘

Kubernetes / cAdvisor / Prometheus / procfs collector
  └─ Pod、进程、CPU、RSS、FD、Socket、重启、OOM 和 Actrix 指标
~~~

图中是逻辑角色，不是固定的 Peer 图。CI 部署的是未接线的 Endpoint 池；
SoakController 在 run 启动时通过 peer 管理 API 按计划接线、结束时拆除。只有当
某个 run 接线了某条边，两个 Endpoint 之间才会有真实 RTC 流量。

### 3.2 Endpoint Pod 形态

每个 `actr-N` Pod 内的控制适配器需要访问本地 Node 的 NetworkEventHandle、RTC
observation 和测试 Fault Registry。因此推荐端点控制适配器与 ActrNode 在同一个
测试进程中：

~~~text
actr-N Pod
  └── actr-soak-endpoint 〔推荐名称；当前兼容 target：actr-soak-agent〕
      ├── ActrNode
      ├── 生产 RuntimeContext / PeerGate / RTC Manager
      ├── Endpoint 控制 API
      ├── 测试业务 Probe Driver
      ├── 测试 Fault Adapter
      └── Snapshot / Hook / Metrics Adapter
~~~

Collector 可以是 sidecar。Endpoint 控制适配器不推荐作为独立进程，因为进程外无法
持有进程内的 RuntimeContext、Manager 和 Fault Registry。

### 3.3 控制面、业务面、观测面

~~~text
控制面：
soak-dashboard / 内嵌 SoakController → Endpoint 控制 API

事件面：
actr-N 控制适配器 → NetworkEventHandle → RecoverySupervisor → RtcPeerController

业务面：
Test Workload → RuntimeContext → PeerGate → PeerTransport
  → RtcConnectionManager → RtcPeerController
  → PeerConnectionSession → WebRTC DataChannel → 对端 Handler

Fault 面：
actr-N 控制适配器 → test-only Fault Registry
  → PeerConnectionFactory 的 Runtime/UDP Socket seam

观测面：
Manager Snapshot + Hook + Fault Metrics
  + Endpoint business metrics
  + Actrix metrics
  + Kubernetes/cAdvisor/procfs metrics
~~~

CI 属于部署面，不属于业务控制面：它负责构建/推送镜像、应用 Kubernetes 资源和等待
健康状态，不向 Dashboard 控制的拓扑注入事件，也不启动浸泡计划。Dashboard 内嵌的
SoakController 不能直接写 Controller 内部状态，也不能绕过 PeerGate 直接操作
DataChannel。

---

## 4. 生产链路与测试复用边界

### 4.1 生产业务链路

~~~text
RuntimeContext
  → Gate::Peer
  → PeerGate
  → PeerTransport
  → RtcConnectionManager
  → RtcPeerController
  → PeerConnectionSession
  → webrtc-rs PeerConnection / DataChannel
  → 远端 InboundRouter / 业务 Handler
~~~

RuntimeContext 是每次业务调用的上下文，不是由 Controller 手工伪造的对象。`actr-N`
端点进程应启动一个测试 workload，让 actr 正常创建上下文，再执行 call、tell 或
send_data_chunk。

PeerGate 是远端发送网关。RuntimeContext 根据 Dest::Peer 选择 PeerGate，由 PeerGate 完成序列化、发送和 pending request 管理。

### 4.2 测试 Fault 的插入点

测试构建时，PeerConnectionFactory 在创建每个新 generation 时把真实 Runtime 包装一层：

~~~text
RtcRuntimeConfig::runtime()
  → webrtc-rs default Runtime
  → test-utils 时可包装为 ScopedFaultRuntime
  → PeerConnectionBuilder.with_runtime(...)
  → 真实 PeerConnection
~~~

因此 DirectedRtcFaultRegistry 不替换 PeerGate、Manager、Controller 或 Session，只在真实 UDP Socket 边界执行丢弃、黑洞、延迟或确定性丢包。

### 4.3 覆盖结论

| 验证内容 | 是否经过生产链路 |
| --- | --- |
| RPC/DataChunk、Offer/Answer、ICE、DTLS、SCTP、DataChannel | 是 |
| Controller recovery、sendable 和 generation | 是 |
| Background/Foreground 和网络事实 | 是 |
| A→B 定向 UDP 故障 | 生产链路 + 测试 Runtime 包装 |
| Offer/Answer/Candidate 故障 | 生产 signaling 链路 + 测试传输装饰器 |
| Pod/CNI/节点和进程故障 | 生产链路 + Kubernetes 外部故障 |

测试 Fault 可以覆盖生产 RTC 控制逻辑，但不单独证明真实 CNI、NAT、MTU、内核缓冲区和运营商网络行为。Nightly 首先使用确定性 Fault，后续增加 Kubernetes 外部网络 profile。

---

## 5. 设计思路与实现方案

### 5.1 设计思路

#### 生产链路优先

测试只负责产生业务、生命周期事实和受控 Fault，不重新实现 RTC 协议。业务必须从测试 workload 进入 `RuntimeContext`，再经过 `PeerGate`、`RtcPeerController` 和真实 DataChannel；恢复必须由生产 `RecoverySupervisor` 和 Controller 完成。

#### 三个平面分离

~~~text
控制面：soak-dashboard / 内嵌 SoakController → Endpoint 控制 API
业务面：Test Workload → RuntimeContext → PeerGate → RTC
观测面：Snapshot/Hook/Metrics → Collector → 结果判定器
~~~

控制面只发动作，业务面只跑真实流量，观测面只读状态。这样可以避免 Controller 直接修改 RTC 内部状态，也能区分“命令已接受”和“业务已经恢复”。

#### 故障按边界分层

- 移动和网络事实通过 `NetworkEventHandle` 进入生产 recovery 入口；
- RTC UDP 和信令消息 Fault 通过 test-only transport/runtime seam 精确注入；
- Pod、进程、节点和 CNI 故障由 Kubernetes 执行；
- Nightly 先使用确定性 Fault，后续用 Kubernetes 外部 Fault 做真实环境对照。

#### 确定性和可重放

每次运行固定 `run_id`、main SHA、镜像 digest、seed 和事件计划。每个动作携带
`action_id` 与 `expected_generation`，Endpoint 控制 API 做幂等处理，失败时可以用
同一份 `plan.json` 重放。

#### 闭环判定

每个测试周期都遵循“注入动作 → 发送业务 → 等待状态 → 检查结果 → 记录证据”的闭环。结果判定器由确定性代码决定 PASS/FAIL，AI 只负责读取证据和给出初步归因。

### 5.2 实现方案

| 模块 | 实现方式 | 关键职责 |
| --- | --- | --- |
| Actr Endpoint（`actr-N`） | 与 actr Node 同进程的 `test-utils/soak` 构建 | 暴露 Endpoint 控制 API，持有 `NetworkEventHandle`、Fault facade、Snapshot/Hook 观察器 |
| Test Workload | `actr-N` 内的测试 Actor/Probe Driver | 通过 `RuntimeContext` 发送 RPC、Tell 和 DataChunk，持续产生双向业务 |
| Soak Dashboard | Kubernetes Deployment | 提供 UI/BFF、持久化运行状态，并提供认证的人工控制入口 |
| Soak Controller | Dashboard 内嵌 | 生成 seed 计划，调用 Endpoint 控制 API，执行事件编排、结果判定器和 artifact 归档 |
| 池租约管理器 | Dashboard 模块 | 为 run 分配独占池子集、排队放不下的 run，并按 profile 执行并发策略（覆盖类可并发、资源类独占） |
| RTC/信令 Fault Adapter | 包装生产 Runtime 和 signaling transport | 调用 `DirectedRtcFaultRegistry`、`DirectedSignalingFaultRegistry`，不改变 Controller/Session 逻辑 |
| Observation Adapter | 读取 Manager Snapshot、Hook 和测试计数器 | 转换为 Endpoint JSON、事件流和 Prometheus 指标 |
| Actrix | Kubernetes StatefulSet | 提供真实 Signaling、STUN/TURN/AIS 服务；Endpoint 通过 Service DNS 访问 |
| Collector | sidecar 或独立 Deployment | 采集 Endpoint、Actrix、Kubernetes、procfs 和日志数据 |
| 结果判定器（Result Evaluator） | Controller 内确定性模块（当前实现目录为 `oracle/`） | 检查业务正确性、恢复时限、Peer 隔离、generation fencing 和资源趋势 |
| CI 部署工作流 | GitHub Actions | 构建/推送不可变镜像，部署 namespace 和 Endpoint 池并等待健康；不得启动测试计划 |

Endpoint 控制调用关系：

~~~text
POST /v1/probes/rpc
  → Test Workload
  → RuntimeContext.call/tell
  → PeerGate
  → RtcConnectionManager / RtcPeerController
  → PeerConnectionSession / DataChannel

POST /v1/events/lifecycle
  → NetworkEventHandle
  → RecoverySupervisor
  → RtcPeerController

POST /v1/faults/rtc 或 /v1/faults/signaling
  → test-only Fault Adapter
  → 生产 Runtime/Signaling transport seam

POST /v1/peers 或 DELETE /v1/peers
  → ActrNode 建立或拆除 Peer 边
  → Peer Snapshot 确认接线结果

GET /v1/peers、/v1/events、/metrics
  → Snapshot / Hook / Metrics Adapter
  → Soak Controller 结果判定器
~~~

实现目录和交付物：

~~~text
testing/actr-soak/
  actr-soak-endpoint/ 〔推荐名称；当前 target：actr-soak-agent〕
  actr-soak-controller/
  protocols/
  workload/
  result-evaluator/ 〔推荐名称；当前实现目录：oracle/〕
  artifact/

deploy/actr-soak/
  namespace.yaml
  actrix-statefulset.yaml
  endpoint-statefulset.yaml
  network-policy.yaml
  rbac.yaml

docker/actr-soak/
  Dockerfile.endpoint 〔推荐名称；当前文件可暂保留 Dockerfile.agent〕

.github/workflows/ci-rtc-k8s-soak.yml
~~~

若独立 Endpoint 进程需要访问当前 crate-private observation，增加 test-only/soak
facade。它只导出只读 Snapshot、Hook、Fault 和资源 observation，不改变正式
recovery、sendable、signaling 或 FFI 语义。

---

## 6. Endpoint 控制 API（旧实现名称：Agent API）

### 6.1 API 的定位

Endpoint 控制 API 是测试命名空间内的远程控制适配层。推荐进程名称为
`actr-soak-endpoint`；当前代码和部署仍使用兼容名称 `actr-soak-agent`，已有代码
也称其为 `Agent API`。它不是 AI Agent，也不是正式 actr SDK 或生产服务 API，
只能调用预先定义的动作，不能执行 shell、eval 或任意内存修改。

### 6.2 API 路由

| Method | Path | 作用 |
| --- | --- | --- |
| GET | /livez | `actr-N` 进程存活 |
| GET | /readyz | Node、ActrId、Actrix 和测试 workload 已就绪 |
| GET | /v1/identity | run、Pod、ActrId、pod_uid、process incarnation |
| GET | /v1/peers | Peer Snapshot 和资源 observation |
| POST | /v1/peers | 接线一条到目标 Endpoint 的 Peer 边（run 作用域、幂等） |
| DELETE | /v1/peers | 拆除本 run 接线的 Peer 边 |
| GET | /v1/events | Hook、状态边沿、Fault 命中和信令 trace |
| GET | /metrics | Prometheus 指标 |
| GET | /v1/metrics | JSON 指标快照，可选 |
| GET | /v1/resources | 进程资源快照（RSS、FD、socket） |
| GET | /v1/diagnostics | Endpoint 诊断快照 |
| POST | /v1/observations/data-chunk | 记录一条 DataChunk 接收证据 |
| POST | /v1/probes/rpc | 通过正式 RPC 业务链路发送探针 |
| POST | /v1/probes/data-chunk | 通过正式 DataChunk 链路发送探针 |
| POST | /v1/events/lifecycle | Background/Foreground |
| POST | /v1/events/network-path | Available/Unavailable/transport |
| POST | /v1/faults/rtc | 安装、更新或解除 RTC Fault |
| POST | /v1/faults/rtc/prepare | 准备一次双侧 RTC Fault 事务（run 作用域、幂等） |
| POST | /v1/faults/rtc/commit | 在两端提交已准备的 RTC Fault 事务 |
| POST | /v1/faults/rtc/rollback | 回滚已准备的 RTC Fault 事务 |
| POST | /v1/faults/signaling | 安装、更新或解除信令 Fault |
| POST | /v1/actions/cleanup | 调用生产 cleanup 入口 |
| POST | /v1/actions/force-reconnect | 调用生产 force reconnect 入口 |

所有写请求携带 run_id、action_id、deadline_ms 和 expected_generation；Endpoint 控制 API 对 action_id 做幂等。

peer 接线写请求是 run 作用域的：run 必须在租约子集归还池之前拆除自己接线的
所有边；Controller 不得向租约外的 Endpoint 接线。

### 6.3 业务探针

RPC：

~~~json
{
  "run_id": "nightly-001",
  "action_id": "rpc-1001",
  "target": "actr-1",
  "route_key": "soak.echo.request",
  "payload_base64": "...",
  "timeout_ms": 5000,
  "sequence": 1024,
  "hash": "sha256:..."
}
~~~

Endpoint 控制适配器内部由测试 workload 使用：

~~~text
RuntimeContext.call(Dest::Peer(peer_b), request, timeout_ms)
  → PeerGate
  → 真实 WebRTC DataChannel
~~~

DataChunk 请求使用 stream_id、sequence、payload size 和 hash，验证乱序、重复、丢失和恢复后的继续发送。

API 返回“动作已接受/执行结果”，不直接把“接口返回 200”当成 RTC 恢复成功。恢复成功必须由后续 Snapshot 和业务探针共同确认。

### 6.4 生命周期事件

~~~json
{
  "run_id": "nightly-001",
  "action_id": "lifecycle-2001",
  "state": "foreground",
  "background_duration_ms": 30000
}
~~~

`actr-N` 控制适配器调用生产入口：

~~~text
NetworkEventHandle.handle_app_lifecycle_changed(...)
  → NetworkEventProcessor
  → RecoverySupervisor
  → RtcPeerController
~~~

网络路径事件使用 NetworkSnapshot 的 Available、Unavailable 和 transport flags。第一阶段不修改 Pod IP，重点验证逻辑网络事实到 ICE/recovery 的生产路径。

### 6.5 安全

- Endpoint 控制 API 只绑定测试 Service 或 Pod 网络；
- NetworkPolicy 只允许 Dashboard 访问控制端口；
- 使用每次 run 的短期 token 或 mTLS；
- Endpoint 进程不持有 cluster-admin；
- 不接受 shell、任意 URL、动态 Rust 代码或任意 protobuf 类型；
- 日志脱敏，不输出 TURN password、AId token、私钥和完整业务 payload；
- 生产镜像不包含 Endpoint 控制 API、test-utils 和 Fault Registry。

---

## 7. 事件与 Fault 设计

### 7.1 移动和网络事件

| 事件 | 注入方式 | 主要验证 |
| --- | --- | --- |
| Background | NetworkEventHandle | 进入后台后的发送和恢复策略 |
| Foreground，1 秒 | NetworkEventHandle | 短后台 |
| Foreground，29.999 秒 | NetworkEventHandle | 30 秒边界前 |
| Foreground，30 秒 | NetworkEventHandle | 长后台阈值边界 |
| Foreground，60 秒 | NetworkEventHandle | recovery timeout 边界 |
| NetworkUnavailable | NetworkSnapshot | 逻辑断网和 pending 处理 |
| NetworkUnavailable → Available | NetworkSnapshot | ICE restart/recovery |
| Unknown → Available | NetworkSnapshot | validated network 唤醒 |
| Wi-Fi ↔ Cellular | transport flags | 网络类型变化 |
| ForceReconnect | NetworkEventHandle | 显式清理并重连 |
| CleanupConnections | NetworkEventHandle | 清理但不自动重连 |
| 重复/乱序监控事件 | source epoch/sequence | stale fencing 和去重 |

当前代码中的后台恢复阈值和 recovery timeout 是实现配置；测试读取实际配置并生成边界，不能在 Controller 中复制一份可能过期的常量。

### 7.2 DirectedRtcFaultRegistry

Registry 的作用是将规则绑定到：

~~~text
(local ActrId, remote ActrId, pc_generation)
~~~

支持的 PacketRule：

~~~text
Pass
Blackhole
DropEvery(n)
Delay(duration)
DeterministicLoss(percent, seed)
~~~

初始化和使用：

~~~text
PeerConnectionFactory.with_directed_faults(local_id, registry)
  → 每个 generation build 时 registry.scoped_runtime(...)
  → ScopedFaultRuntime
  → ScopedFaultUdpSocket
  → 对真实 UDP 收发应用规则并记录命中
~~~

单进程测试可使用 set_link_rule：

~~~rust
registry.set_link_rule(
    (&peer_a, generation_a),
    (&peer_b, generation_b),
    LinkDirection::AToB,
    PacketRule::Blackhole,
);
~~~

多 Pod 中 Registry 不共享内存，因此 A→B 需要两个 Endpoint 控制动作：

~~~text
actr-0：local=actr-0, remote=actr-1，设置 outgoing=Blackhole
actr-1：local=actr-1, remote=actr-0，设置 incoming=Blackhole
Controller：等待两端 hit，再开始 recovery 结果判定
~~~

解除故障时两端都恢复为 Pass。新 generation 不隐式继承旧 generation 的 Fault。

### 7.3 Fault Registry 的覆盖边界

可以覆盖：

- 真实 PeerConnection 上的 UDP 黑洞、延迟和确定性丢包；
- Offer/Answer 后的真实 DataChannel 业务失败；
- 真实 Controller 对 socket 异常、sendability 和 recovery 的处理；
- generation 级 stale Fault 不污染新连接。

不能单独覆盖：

- WebSocket 信令，需要 DirectedSignalingFaultRegistry；
- Pod 被杀、节点 drain、OOM，需要 Kubernetes；
- CNI、NAT、MTU、内核 socket 缓冲区，需要外部网络 profile；
- 物理无线网络和系统后台，需要真机。

### 7.4 信令 Fault

优先覆盖：

- 丢第一次 IceRestartOffer；
- 丢 IceRestartRequest；
- 延迟或重复 Answer；
- 丢 Candidate；
- 重复 RoleAssignment；
- 信令连接断开但 RTC 仍保持；
- SDP queue full 后恢复发送。

Fault 类别、方向、exchange、attempt 和 ordinal 写入事件 trace，便于重放。

### 7.5 Pod 和 Actrix 生命周期

通过 Kubernetes 执行：

~~~text
正常删除：grace-period=15
强制删除：grace-period=0
同 ActrId 重建
新 ActrId 启动
Actrix Pod 重启
Endpoint rolling restart
~~~

每个动作记录 pod_uid、pod_ip、node_name、ActrId、process incarnation 和 action_id。

---

## 8. 固定 Seed 和随机事件调度

### 8.1 Seed 组成

~~~text
run_seed
  ├── topology_seed
  ├── peer_seed
  ├── traffic_seed
  ├── event_seed
  ├── duration_seed
  ├── rtc_fault_seed
  ├── signaling_fault_seed
  └── lifecycle_seed
~~~

使用显式可复现的 PRNG。每次动作生成前把计划写入 plan.json，不依赖 Pod 返回顺序、系统时间或未记录的随机数。`topology_seed` 驱动生成记录在 `plan.json` 中、run 启动时接线的 Peer 图；Peer 图是计划数据，不是部署约束。

### 8.2 调度约束

- 每个 Endpoint 最多同时执行一个破坏性动作；
- 同一个 Peer 必须等待上一动作进入恢复或超时状态后再注入下一动作；
- Fault 安装和解除成对出现；
- Pod 删除不能和同一 Pod 的 Fault commit 同时执行；
- 每个 coverage window 必须覆盖关键事件类别；
- 事件生成器可以随机，结果判定器和恢复 deadline 不能随机；
- 任何动作都记录 target、direction、generation、开始时间、结束时间和结果。

### 8.3 推荐事件权重

初始权重用于获得均衡覆盖，后续按历史失败率调整：

| 类别 | 初始权重 |
| --- | ---: |
| 正常业务窗口 | 45% |
| Background/Foreground | 15% |
| NetworkUnavailable/Available | 10% |
| RTC Fault | 10% |
| Signaling Fault | 8% |
| 在途业务/关闭 lane | 5% |
| Endpoint 重启 | 4% |
| Actrix 重启 | 2% |
| 网络外部 Fault | 1% |

每 30～60 分钟强制进行一次 coverage 检查，避免随机分布长期漏掉稀有事件。

---

## 9. 拓扑和业务负载

### 9.1 拓扑 profile

拓扑 profile 是接线配方。SoakController 在 run 启动时把 profile 应用到租到的
池子集上；不同 profile 不需要不同的部署。

| Profile | 拓扑 |
| --- | --- |
| pair | `actr-0` 和 `actr-1`，单 Peer 边 |
| small | `actr-0` 到 `actr-N`，固定 ring + 随机交叉 Peer |
| multi-peer | `actr-0` 作为 hub，`actr-1` 到 `actr-N` 作为 leaf，每个端点限制 4～8 个活跃 Peer |
| restart | `actr-0` 到 `actr-N`，周期性重建身份 |
| relay | 全部连接必须走 TURN relay；Controller 必须依据 Endpoint 诊断信息验证 relay 路径，不得假定 |

不建议 50 个 Endpoint 全互联，避免 O(N²) 连接数主导测试资源。每个 profile 必须把接线后的 Peer 图记录在 `plan.json` 中。

### 9.2 业务模型

所有 payload 包含：

~~~text
run_id、cycle_id、source、target、direction、sequence、request_id、payload_hash
~~~

| 业务 | 初始频率 | 目的 |
| --- | ---: | --- |
| 双向小 RPC | 每方向 1 req/s | 连通性、延迟和响应匹配 |
| 双向 DataChunk | 每方向 1/10s | Stream lane 和恢复后继续发送 |
| 200 KiB 分片 RPC | 每方向 1/30s | 分片、重组、乱序和预算 |
| 1 MiB burst | 每 5 min 轮转 | 压力后资源回落 |
| 在途 RPC | 事件前随机产生 | timeout、取消和 recovery 竞争 |

故障期间允许业务在 deadline 内失败，但不允许无限 pending、跨 Peer 错误响应、重复执行或恢复后永久不可用。

---

## 10. 历史问题回归

| Issue/PR | 浸泡测试动作 |
| --- | --- |
| [#583](https://github.com/Actrium/actr/issues/583) | 远端 Endpoint 静默死亡，新 ActrId 启动 |
| [#608](https://github.com/Actrium/actr/issues/608) | stalled Controller 与其他 Peer 并发 signaling |
| [#609](https://github.com/Actrium/actr/issues/609) | SDP queue full、发送失败和唤醒 |
| [#610](https://github.com/Actrium/actr/issues/610) | 关闭 lane、旧 DataChannel 和后续业务 |
| [#604](https://github.com/Actrium/actr/issues/604) | pair contract 并发恢复和重复运行 |
| [#492](https://github.com/Actrium/actr/issues/492) | setup/send admission 超时 |
| [#270](https://github.com/Actrium/actr/issues/270) | ICE/UDP socket、FD 长期趋势 |
| [#297](https://github.com/Actrium/actr/issues/297) | RTC timeout 与 WebSocket fallback |
| [#339](https://github.com/Actrium/actr/issues/339) | RPC timeout/cancellation 与 recovery |
| [#359](https://github.com/Actrium/actr/issues/359) | stale DataChannel sendable 误报 |
| [#363](https://github.com/Actrium/actr/issues/363) | answerer 短后台和 restart request |
| [#383](https://github.com/Actrium/actr/issues/383) | 长后台、signaling reconnect 和首次 foreground |
| [#403](https://github.com/Actrium/actr/issues/403) | signaling socket supervisor ownership |
| [#426](https://github.com/Actrium/actr/issues/426) / [#622](https://github.com/Actrium/actr/pull/622) | per-peer recovery completion |
| [#436](https://github.com/Actrium/actr/issues/436) / [#508](https://github.com/Actrium/actr/issues/508) | ICE restart offer/request 丢失 |
| [#465](https://github.com/Actrium/actr/issues/465) | TURN credential rotation |
| [#503](https://github.com/Actrium/actr/issues/503) | 被动关闭 WS 后 tell/stream 自愈 |
| [#567](https://github.com/Actrium/actr/issues/567) | 后台 stale UDP、rebind 和 ICE restart |
| [#581](https://github.com/Actrium/actr/issues/581) | broadcast lagged、close cleanup 和 pending |
| [#594](https://github.com/Actrium/actr/pull/594) / [#596](https://github.com/Actrium/actr/pull/596) | Offerer/Answerer Soft recovery |

---

## 11. Metrics、Snapshot、Hook 和 Evidence

### 11.1 当前已经存在的能力

目前不是“没有 Metrics”，而是指标分散在不同层：

| 层级 | 已有能力 | 当前用途 |
| --- | --- | --- |
| RTC Manager | peer_count、per-peer Snapshot、live_lanes、inbound dropped | Manager observation，主要供测试适配 |
| Snapshot | status、role、pc_generation、revision、sendable、network path、原因、signaling dropped、mailbox overflows | 状态断言 |
| Hook | Connecting、Connected、Disconnected、signaling 生命周期 | 边沿和时间线 |
| RTC Fault | outgoing/incoming hit | 判断 Fault 是否真正生效 |
| BuildThrottle | active、peak、acquisitions、contention | 并发构建资源 |
| Runtime Fault Probe | task、UDP socket、TCP、DNS、sleep 和注入 Fault 计数 | test-only 诊断 |
| Evidence | 状态、Hook、Fault 和 signaling trace 合并 | 失败重放 |
| Actrix | 系统指标、信令队列指标和部分 metrics endpoint | Actrix 健康和负载 |
| Kubernetes | CPU、memory、restart、OOM、throttling、Pod 状态 | 系统资源趋势 |

因此，方案需要新增的是 Endpoint 聚合层和长期趋势指标，不是重新实现所有底层观测。

### 11.2 当前缺口

- 还没有统一的测试 Endpoint /metrics；
- RPC/DataChunk 成功率、延迟、payload hash 和 duplicate 计数需要由 Probe Driver 统计；
- recovery attempt、duration、outcome 和 deadline 需要由 Endpoint 事件适配层关联；
- 当前公开 Snapshot 不直接暴露全部内部 reducer 字段，例如 ICE generation；若测试必须判断，需要 test-only observation/evidence，不扩展正式 FFI；
- 不能使用历史文档中已删除的 Actr-wide total_outstanding_bytes 作为现有指标；
- Actrix exporter 是否在目标镜像启用，必须在 preflight 中验证。

### 11.3 推荐 Endpoint 指标

指标前缀 `actr_soak_agent_` 为兼容当前实现而保留，不改变端点命名约定。拓扑和
Dashboard 使用 `actr-0`、`actr-1`、……，`peer` 表示某条连接的远端一侧。

Counter：

~~~text
actr_soak_rpc_sent_total
actr_soak_rpc_success_total
actr_soak_rpc_timeout_total
actr_soak_data_chunk_sent_total
actr_soak_data_chunk_hash_error_total
actr_soak_recovery_total{kind,outcome}
actr_soak_fault_hits_total{kind,direction}
actr_soak_hook_events_total{event}
actr_soak_agent_actions_total{kind,result}
~~~

Gauge：

~~~text
actr_soak_peer_count
actr_soak_pending_requests
actr_soak_active_lanes
actr_soak_recovery_obligations
actr_soak_fault_leases
actr_soak_build_throttle_active
actr_soak_build_throttle_waiters
~~~

Histogram：

~~~text
actr_soak_rpc_latency_seconds
actr_soak_recovery_duration_seconds
actr_soak_reconnect_duration_seconds
~~~

### 11.4 三类观测的区别

- Snapshot：回答“现在是什么状态”；
- Hook：回答“状态按什么顺序变化过”；
- Metrics：回答“数量、延迟和资源趋势如何”；
- Evidence：把一次动作的计划、状态、Hook、Fault、业务和 K8s 事件串成可重放时间线。

### 11.5 资源采集

Kubernetes 可以直接收集 Pod CPU、内存、重启、OOM 和 throttling。FD、Socket、线程和 RSS 需要：

- `actr-N` 进程自采集 procfs；或
- Pod 开启 shareProcessNamespace，由受限 sidecar 读取目标进程 procfs；或
- 由节点级 collector 采集并按 Pod/容器关联。

资源指标不写入 actr 业务状态机。只读 observation 和系统采集应与 Fault 控制分离。

---

## 12. 结果判定器和结果判定

### 12.1 立即失败

- Endpoint 控制适配器、Actr 或 Actrix panic、hang 或非预期退出；
- RPC/DataChunk 超过 deadline；
- response 的 request_id、source、target、sequence 或 hash 错误；
- 非目标 Peer 被 Fault 或恢复动作影响；
- stale generation 修改当前 generation；
- Fault 已声明但没有命中；
- Pod 出现非预期 OOMKilled、eviction 或自动重启；
- Controller 自身 retry 或违反动作约束；
- artifact 脱敏闸门失败，或在已导出的 artifact 中检出 secret、未脱敏的
  完整 payload（见 §14.1）。

### 12.2 业务正确性

每个成功或允许失败的请求写入结果表：

~~~text
action_id、request_id、source、target、direction、sequence
sent_at、received_at、response_at、error、payload_hash
~~~

判定规则：

- 无故障窗口：业务成功率必须达到 profile 阈值；
- 故障窗口：允许在 deadline 内失败，但恢复后新业务必须成功；
- 不允许同一 request 被重复执行；
- 不允许旧 Peer 的响应进入新 Peer 或新 generation；
- 一个 Peer 故障不能让其他 Peer 的业务长期失败。

### 12.3 Recovery 判定

Soft Recovery：

~~~text
Peer 进入 Recovering 或不可发送
  → 预期 signaling/ICE 事件完成
  → sendable 恢复
  → 双向 RPC/DataChunk 成功
~~~

Hard Recovery：

~~~text
旧 Session/lane 退休
  → 新 pc_generation 建立
  → Offer/Answer、ICE、DTLS、SCTP 完成
  → sendable 恢复
  → 双向业务成功
~~~

不要求 A、B 两端 generation 数值相等，只要求每端相对于自身基线满足预期单调关系。

恢复 deadline 使用运行时配置和实际事件时间，不在 Controller 中复制固定 sleep。当前默认 recovery budget 作为初始门限，profile 可显式覆盖。

### 12.4 资源收敛

每个 recovery cycle 后进入 quiescence，再检查：

~~~text
pending_requests = 0 或只保留明确在途请求
BuildThrottle.active = 0
旧 session/lane/fault lease = 0
recovery obligation = 0
~~~

收尾 quiescence 检查同时是池的交接闸门：上述值清零（或只剩明确在途请求）之前，
租约子集不得归还空闲池。

长时间使用 warm-up 后窗口的中位数、P95 和线性斜率：

- FD、Socket、task、pending 不允许持续正斜率；
- RSS 不要求回到启动值，但不能在相同负载和周期下持续增长；
- CPU 结合稳态使用率和 throttling 判断；
- 绝对阈值放在 profile 配置中，不写死在随机生成器里。

### 12.5 结果分类

~~~text
PASS
FAIL_SUT
FAIL_HARNESS
INVALID_INFRASTRUCTURE
KNOWN_ISSUE
CANCELLED
~~~

---

## 13. Kubernetes 落地

### 13.1 Namespace

~~~text
actr-soak-<run-id>
  ├── actrix-0
  ├── Endpoint 池：actr-0 ... actr-N（未接线）
  ├── soak-dashboard
  ├── collector
  └── artifact-uploader
~~~

namespace 内隔离 realm、ActrId、credential、Fault、NetworkPolicy 和 artifact。

### 13.2 Workload

| 组件 | Kubernetes 类型 | 说明 |
| --- | --- | --- |
| Actrix | StatefulSet，初始 1 副本 | Signaling、STUN、TURN、AIS |
| Actr Endpoint（`actr-N`） | StatefulSet | 稳定 ordinal，Pod 重建后 IP 仍可能变化；池成员启动时不接线，Peer 边在 run 启动时建立 |
| Soak Dashboard | Deployment | UI/BFF、人工控制、内嵌 Controller 和运行状态持久化 |
| Collector | sidecar 或独立 Deployment | metrics、procfs、日志 |

StatefulSet 的 ordinal 只用于角色标识，不把 Pod IP 当作永久身份。

### 13.3 端口和地址

端口以实际镜像配置为准，建议：

~~~text
Actrix HTTP/Signaling：TCP 8080
Actrix STUN/TURN：UDP 3478
TURN relay range：专用 UDP 范围
Endpoint 控制 API：TCP 9090
Endpoint metrics：TCP 9091
~~~

STUN/TURN 和 relay bind.ice.advertised_ip 使用稳定的 LoadBalancer、MetalLB 或节点可达地址。若使用 Pod IP，Actrix 重建后必须重新渲染地址并重新注册。

### 13.4 NetworkPolicy

~~~text
Dashboard → `actr-N`：TCP 9090（Endpoint 控制 API）
Dashboard → Kubernetes API：TCP 443/6443（读取 Pod、Events、日志、metrics；
              仅限批准的生命周期动词）
Collector/Prometheus → `actr-N`：TCP 9091（Endpoint metrics 抓取）
Collector/Prometheus → Actrix：TCP metrics 端口（Actrix metrics 抓取）
`actr-N` → Actrix：TCP HTTP/Signaling、UDP STUN/TURN 3478
`actr-N` ↔ Actrix：TURN relay UDP 端口段（relay allocation 的中继媒体，
            relay profile 必需）
`actr-N` ↔ `actr-N`：WebRTC ephemeral UDP
所有 Pod → kube-dns：TCP/UDP 53
其他流量：拒绝
~~~

矩阵必须列出设计依赖的每一条流量；缺少 allow 规则导致的是 harness 故障而非
SUT 故障，归类 INVALID_INFRASTRUCTURE。Dashboard 到 Kubernetes API 的出向
只允许指向 API server；TURN relay 端口段必须是配置的专用段，不得放宽为
大范围 UDP 放行。

池模型依赖 Endpoint 之间的 UDP 放行；不得按 run 收窄，因为任何池成员都可能被
计划接线到任何其他成员。

第一阶段只使用 Endpoint Fault Registry 和生命周期事件；真实 CNI delay/loss/partition 作为后续 profile。

### 13.5 RBAC 和资源

Dashboard 只获得本次 namespace 内 Pod、Events、日志和 metrics 的必要读取权限。
不授予 cluster-admin、跨 namespace 写权限和任意 Secret list 权限。

起始资源仅用于 preflight，需根据基线测量校准：

~~~text
Actrix：2 CPU / 2 GiB
Endpoint：2 CPU / 2 GiB
Dashboard（含内嵌 Controller）：1 CPU / 512 MiB
Collector：0.5 CPU / 512 MiB
~~~

正式长跑必须设置 requests、limits、节点选择、Pod anti-affinity 和专用资源配额，避免 CPU contention 造成误报。

---

## 14. Artifact、重放和 GitHub Actions

### 14.1 Artifact

~~~text
<run-id>/
  ├── manifest.json
  ├── git.json
  ├── plan.json
  ├── timeline.jsonl
  ├── operations.jsonl
  ├── business-results.jsonl
  ├── endpoint-snapshots/*.jsonl 〔当前兼容路径：agent-snapshots/〕
  ├── endpoint-metrics/*.prom 〔当前兼容路径：agent-metrics/〕
  ├── actrix-metrics/*.prom
  ├── k8s/events.json
  ├── k8s/objects.json
  ├── logs/
  ├── summary.json
  ├── junit.xml
  └── reproduce.sh
~~~

必须记录 main SHA、镜像 digest、配置摘要、seed、Pod UID、ActrId、process incarnation、action_id 和 collector 版本。

Pod 删除后 artifact 不能依赖 emptyDir，使用 S3/MinIO、PVC 或集中日志系统。

脱敏是确定性闸门，不是尽力而为的过滤。独立的脱敏步骤必须在证据收集之后、
任何 artifact 上传、导出或组装 Claude Code 输入之前运行。它扫描日志、快照和
证据 JSON 中的 TURN password、AId token、Endpoint 控制 token、私钥、带凭据
URL 和完整业务 payload，使用固定模式加 manifest 中记录的每 run 凭据清单；
基于模式匹配，结果确定，不引入模型。该闸门是 artifact 上传验收的一部分：

- 闸门通过前 artifact 不得离开 namespace 或 PVC；闸门结果本身也作为 artifact
  落盘（`redaction.json`）；
- 闸门失败（扫描器错误）或发现无法脱敏的内容时，阻断 artifact 导出，且本 run
  不得判 PASS；归类 FAIL_HARNESS（证据管线不安全），不得归为 FAIL_SUT；
- 已导出的 artifact 中检出 secret 属于立即失败（§12.1），并触发相关凭据轮换。

reproduce.sh 至少能：

1. 使用同一 main SHA 和镜像 digest；
2. 使用同一 plan.json；
3. 按 `plan.json` 重新接线相同的 Peer 图，并使用相同配置；
4. 只重放失败动作窗口；
5. 输出相同的 action_id、generation 和结果判定器结果。

### 14.2 Workflow

默认部署工作流将镜像/部署与测试执行分离：

~~~text
CI（构建/部署工作流）
  → checkout 固定 SHA
  → cargo fmt 和构建测试镜像
  → 推送 Endpoint、Actrix 和 Dashboard 镜像并记录 digest
  → 创建/更新 namespace、Secret、RBAC 和 NetworkPolicy
  → 部署 Actrix、未接线的 Endpoint 池、Dashboard 和采集器
  → 只等待 Pod/Dashboard 健康
  → 停止；不创建 Controller Job，不启动浸泡计划

Dashboard（操作员/API）
  → 预览并启动一次测试
  → 租用池子集并接线计划中的 Peer 图
  → 内嵌 SoakController 执行 Endpoint preflight 和业务流量
  → 注入生命周期、网络、RTC/信令事件
  → 收集 Endpoint、Actrix 和 Kubernetes 证据
  → 执行结果判定器，将 artifact 写入 Dashboard PVC
  → 通过 quiescence 闸门，把租约子集归还池中
  → 操作员查看日志、结果判定器输出和 artifact
~~~

Nightly：

~~~text
Dashboard 调度器或操作员触发
北京时间每天 23:00，运行 8 小时
~~~

Weekend：

~~~text
Dashboard 调度器或操作员触发
北京时间周五 23:00，运行 48 小时
~~~

实际时间以 Dashboard 的 run manifest 为准。

ARC 只负责创建 GitHub Actions Runner Pod，不负责 RTC Fault、Endpoint 生命周期或结果判定器。Runner 需要 pinned kubectl、容器工具、Rust 缓存和 artifact 客户端。

---

## 15. Claude Code 初步分析

Claude Code 在 artifact 上传后运行，不参与业务控制闭环。

输入：

- summary.json、plan.json、timeline.jsonl；
- Endpoint Snapshot、Hook、Fault 和业务结果；
- Actrix metrics；
- Kubernetes events、Pod describe 和资源趋势；
- 脱敏日志和 reproduce.sh。

输出：

- SUT、Harness、Infrastructure、Flaky、Known Issue 分类；
- 第一次异常事件和时间线；
- 受影响 Endpoint、Peer、generation、exchange；
- 可能关联的历史 Issue；
- 缺失证据、置信度和最小重放命令。

安全约束：

- AI 不改变结果判定器结果；
- AI 不自动 retry 失败 run；
- AI 不修改阈值、Kubernetes 对象或生产 Secret；
- AI 输出不包含敏感 token；
- 只有人工确认后才进入修复和重放流程。

人工闭环：

~~~text
AI 初步归因
  → 人工确认
  → 修复代码或环境
  → 原 seed 重放
  → 固定回归测试
  → Nightly 8 小时
  → Weekend 48 小时
~~~

---

## 16. Profile 和实施阶段

### 16.1 Profile

| Profile | 拓扑 | 时长 | 目的 | 池占用 |
| --- | --- | ---: | --- | --- |
| preflight | 1 Actrix + 2 Endpoint | 10 min | 镜像、网络、API 和 artifact | 可并发 |
| pair-event | 2 Endpoint | 30～60 min | 事件、RTC/信令 Fault | 可在不相交子集上并发 |
| process-churn | 2 Endpoint | 30～60 min | SIGTERM、SIGKILL、身份重建 | 可在不相交子集上并发 |
| multi-peer | hub + 9 leaf | 2 h | Peer 隔离、并发 recovery | 可在不相交子集上并发 |
| nightly | 2～10 Endpoint | 8 h | 夜间长期稳定 | 独占整个池 |
| weekend | 10～50 Endpoint | 48 h | 周末资源和恢复浸泡 | 独占整个池 |
| release | 50～100 Endpoint | 8～24 h | 发布门禁 | 独占整个池，跑前滚动重建 |

steady 和 recovery 分开报告：steady 不注入破坏性 Fault，recovery 周期性注入并恢复。放不进空闲池的 run 在 Dashboard 队列中等待。

### 16.2 实施阶段

1. **Phase 0：基线和 preflight**
   固定 SHA、镜像、Actrix 地址、namespace、artifact 和无故障双向业务。

2. **Phase 1：最小 Endpoint 控制适配器**
   实现 readiness、identity、RPC/DataChunk Probe、peers 和最小 events。

3. **Phase 2：事件和确定性 Fault**
   接入 NetworkEventHandle、RTC/信令 Registry、幂等、generation fencing 和历史 Issue 场景。

4. **Phase 3：多 Pod 和生命周期**
   接入 StatefulSet、Endpoint 池与运行时 Peer 接线/租约、双端 Fault commit、Pod/Actrix restart、资源采集和结果判定器。

5. **Phase 4：Nightly 8 小时**
   加入固定 seed 随机事件、coverage 检查、retry=0 和 Claude Code 分析。

6. **Phase 5：Weekend 48 小时和外部网络**
   增加 relay、credential rotation、rolling restart、NetworkPolicy/Chaos Mesh/tc-netem 对照。

浸泡 Endpoint 只保留生产 ActrNode 路径，测试接缝（Fault Registry、预注册凭证）由
test-utils 注入；早期的 TestHarness remote 模式直接删除，不作为兼容路径保留——池
模型下只有一种 Endpoint 类型。当前 node 模式部署仍在启动时通过每个 Endpoint 的
`remote-peers` 配置接线；迁移到池模型时新增 peer 管理写 API 和 Controller 侧接线，
node 启动期接线可作为单拓扑 namespace 的短暂过渡，池模型落地后与启动接线配置面
一并删除。

---

## 17. 必需资源、风险与完成定义

### 17.1 必需资源

- 自建 Kubernetes 集群和测试 namespace；
- GitHub Actions ARC Runner；
- pinned kubectl、容器构建和推送能力；
- GHCR 或内部镜像仓库；
- Actrix、Endpoint、Controller 和 Dashboard 镜像；
- Dashboard 用于运行状态和证据的 PVC；
- S3/MinIO/PVC artifact 存储；
- DNS、Pod-to-Pod UDP、STUN/TURN relay 端口；
- 测试 realm、短期 credential 和 Secret 管理；
- 稳定 advertised address 或 Pod IP 重渲染策略；
- 专用节点池、资源配额和 anti-affinity。

建议增加 Prometheus/Grafana、Loki/Vector/OTLP、cAdvisor、Kube State Metrics、namespace TTL janitor、Rust/容器缓存和 Claude Code OIDC。

### 17.2 风险控制

| 风险 | 控制措施 |
| --- | --- |
| 测试 Fault 与真实网络脱节 | Pass 基线 + Kubernetes 外部 Fault 对照 |
| Fault 没有真正作用到 WebRTC | Factory wiring contract test + hit counter |
| Endpoint 控制适配器绕过生产业务链路 | 只允许测试 workload 调用 RuntimeContext |
| old generation 污染新连接 | expected_generation、generation-scoped Fault lease |
| K8s CPU contention 造成假失败 | 专用节点、requests/limits、资源分类 |
| Pod 删除后证据丢失 | 集中 artifact/log storage |
| API 变成生产攻击面 | test-only image、NetworkPolicy、短期 token、无 shell |
| 随机失败无法重现 | 固定 seed、plan.json、镜像 digest、reproduce.sh |
| AI 误判 | 结果判定器与门禁由确定性代码决定 |
| Actrix 地址随重建变化 | 稳定 advertised address 或重渲染并重新注册 |

### 17.3 完成定义

- main SHA、镜像 digest、配置和 seed 被完整记录；
- 真实 Actrix 与多个 `actr-N` Pod 可完成双向 RTC 业务；
- RPC/DataChunk 经过 RuntimeContext、PeerGate 和真实 DataChannel；
- 生命周期事件经过 NetworkEventHandle；
- RTC/信令 Fault 有安装、命中、解除和 generation 证据；
- 多 Pod A→B Fault 可在两端一致建立；
- Pod/Actrix 重启可执行并可重放；
- 单 Peer 故障不阻塞其他 Peer；
- Snapshot、Hook、业务、Actrix、Kubernetes 结果可按 action_id 关联；
- pending、session、lane 和 recovery obligation 有收敛判断；
- FD、Socket、RSS、CPU 和内存有长期趋势；
- Nightly 8 小时和 Weekend 48 小时 profile 可执行；
- retry=0、backoffLimit=0；
- 失败 artifact 在 Pod 删除后仍可获取；
- Claude Code 只做初步分析，人工修复后可按原 seed 重放；
- 测试 namespace、Pod、Job、PVC 和临时 Secret 可自动清理。
- CI 的镜像/部署工作流不会启动 Dashboard 控制的测试；
- Dashboard 是每个 namespace 的唯一测试控制者；
- Peer 图通过 peer 管理 API 接线和拆除，并记录在 plan.json 中；
- 池子集独占租用，只有通过 quiescence 闸门后才归还；
- 覆盖类 profile 可在不相交子集上并发，Nightly/Weekend/release 独占整个池；
- 池可按计划滚动重建，重建不丢失 artifact。

---

## 18. 参考资料

- [Kubernetes Jobs](https://kubernetes.io/docs/concepts/workloads/controllers/job/)
- [Kubernetes StatefulSets](https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/)
- [Kubernetes NetworkPolicy](https://kubernetes.io/docs/concepts/services-networking/network-policies/)
- [Kubernetes Downward API](https://kubernetes.io/docs/concepts/workloads/pods/downward-api/)
- [GitHub Actions self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners)
- [Chaos Mesh NetworkChaos](https://chaos-mesh.org/docs/next/simulate-network-chaos-on-kubernetes/)
