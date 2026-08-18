# 术语

本表只列现行含义。已废弃的旧词（GraphRoot 作为图安全根、逐消息权限、动态 Capability 交集、Waiver 改色、四值 Claim Verdict 作为顶层颜色）不再作为规范用语。

| 术语 | 含义 |
| --- | --- |
| Actr | Actor 通信、身份、运行与协议框架 |
| Actro | 以 Tab 承载 Actor 应用的宿主 |
| Rill | 受约束的 Guest / Host 工作负载模型 |
| Actor | 唯一规范节点；宏观通信与执行边界 |
| ActorId | 实例稳定、可认证身份 |
| ActorState / ActorStateId | 当前实例事实及其内容根 |
| Actor 主体 | 用户在品牌 / 软件层识别的对象 |
| Actor 实例 | `(节点应用 + 配置) + 宿主环境` |
| Principal / PrincipalId | 使用主体及其外部不透明标识 |
| Publisher / Provider / Operator | 发布、提供服务或运营基础设施的主体；不是 Principal |
| Workload | Actor 内的业务计算 |
| Host 软件 | 与 Workload 一起构成节点应用、提供 API 与执行点的软件 |
| 宿主环境 | 软件包之外的 OS、硬件、TEE、管理员边界等 |
| Manifest | 开发者静态描述；只是有效能力声明的输入 |
| 有效能力声明 | 实际可能效果的闭合上限 |
| Transfer / Retain | V1 仅有的两种规范效果 |
| Capability | 绑定 `ActorId + ActorStateId` 的授权凭证 |
| 数据输入授权 | 数据是否进入应用；与颜色正交 |
| 风险接受 | 允许较弱状态运行；不改事实和颜色 |
| Claim / Evidence | 待验证陈述，以及用来核实它的材料 |
| Verifier | 按规则、证据和用户策略核实状态的一方 |
| TrustPolicy | 版本化的信任与行为模式规则 |
| ActorAssurance | 针对准确 ActorStateId 的节点结论 |
| ApplicationStructure | 无根的有效依赖连通组 |
| ApplicationStateId / ApplicationAssurance | 应用当前状态承诺及其汇聚结论 |
| 状态权威 | 某修订期内唯一可认证的当前状态发布者 |
| 保障挂起 | `ASSURANCE_SUSPENDED`：隔离应用数据，但不等于停机 |
| 有效 Actr 通道 | 端到端加密、完整性、双向绑定双方 ActorId + ActorStateId 的通道 |
| Realm / Signaling / TURN | 发现、协调或密文中继设施；不是明文端点 |
| Workspace | 数据空间；不自动是 Actor，同 Principal 不自动授权 |
| TCB | 失陷会破坏相应保障的组件集合 |
| CONTROLLED / UNKNOWN / UNCONTROLLED | 出口事实，不是产品第四色 |
| 绿 / 黄 / 红 | Actro 顶层隐私标记；绿只保证当前就绪切面 |
| 数据世代 | 应用数据取得时的 `ActorStateId`；旧数据适用取得时声明与当前声明的交集 |
| Ready | 已发布快照仍对应当前指针，且全部图成员 `ACTIVE`；展示颜色的前提 |
| EncGraph | 应用图的规范字节编码；对带标签节点／边集合单射 |
| Depends / LocalContinue | 无向有效依赖；不依赖变化节点时允许局部继续，但不得展示应用绿色 |
| 评定上下文 | `ActorStateId` + 策略版本 + 规则集；此范围内多份结论取最弱项 |

## 几组不要混用的词

- **TCB** 是「必须信赖它」；**CONTROLLED** 是「当前这项出口有充分证据」。
- **Security** 是广义安全属性；**Privacy** 是谁能接触、保留和输出数据；**Assurance** 是凭什么相信某属性在给定作用域成立。绿黄红优先表达隐私保障。
- **加密** 保护途中或静态介质；端点拿到明文后能做什么，由能力和执行点决定。
- **Attestation** 提供环境 / 测量证据；**Authorization** 才决定是否放行。
- **Native / Wasm** 是执行技术，不是颜色。

## 规范用语

- 必须 / 禁止：不满足就不符合本模型；
- 应该 / 不应该：偏离时必须记录理由和影响；
- 可以：不改变安全语义的可选实现。

白皮书式叙述和产品文案不是额外要求。
