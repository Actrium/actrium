# Actrium 安全与隐私

数据只能在身份可证、能力显式、执行受限、路径闭合、证据新鲜且结论可执行的 Actor 图中流动。任一必要条件不可证，系统就不得继续把它称为「隐私保障已验证」。

这份目录只保留现行结论。它判断的是**一张应用结构在当前就绪切面、证据和用户策略下，是否有充分依据确认所有可能出口都受控**；不是给应用名称颁发永久证书，也不是当前产品已经部署的能力清单。

```text
Specified → Implemented → Tested → Attested → Deployed
```

后一个状态不得仅因前一个状态存在而自动成立。下文描述的是已冻结的目标模型，成熟度为 `Specified`，除非某条另行给出实现证据。

## 一眼能用的结论

- 安全结论绑定应用**当前就绪切面**，状态变化后旧结论立即失效。绿色不表示曾经存在一个全图同时世界。
- 应用数据带**取得世代**。新配置只作用于新数据。
- 同一评定上下文的多份结论取**最弱项**，不能挑选较绿的验证者。
- Actor 是唯一规范节点。屏幕、文件、对象存储和外部 API 默认是能力目标，不因收到数据就入图。
- V1 只计算两种效果：`Transfer(target)` 与二元 `Retain`。
- 应用图由有效 Actor 依赖形成，无根；图只确定「谁必须一起评」，不做加权评分。
- 顶层只有绿 / 黄 / 红。弱项不能被其他强项抵消。用户风险接受只允许运行，不改颜色。
- 数据输入授权（摄像头、麦克风、文件）与颜色正交。
- Actor 之间必须使用有效 Actr 通道；通道失败即禁止通信，不能靠降级换通行。

## 阅读路径

由浅入深，按需停下：

| 读者 | 从哪里读 |
| --- | --- |
| 第一次接触 | [总览与主张边界](01-overview.zh.md) |
| 要掌握模型 | [原则、威胁与语义](02-model.zh.md) |
| 要实现协议或 Host | [协议与执行](03-protocol.zh.md) |
| 做 Actro 产品 / UX | [产品标记与交互](04-product.zh.md) |
| 做验证、审计或形式化 | [保障、证明与成熟度](05-assurance.zh.md) → [形式模型](formal/model.zh.md) |

术语见[术语表](../../../GLOSSARY.zh.md)「安全与隐私模型」一节。

规范用语：

- 必须 / 禁止：不满足就不符合本模型；
- 应该 / 不应该：偏离时必须记录理由和影响；
- 可以：不改变安全语义的可选实现。

白皮书式叙述和产品文案不是额外要求。

## 与产品文档的关系

Actro v1 产品稿写 Tab / Front / 交付与验收，安全语义以本目录为准：

- [安全与隐私](../../../product/actrium-v1/09-security-and-privacy.zh.md)
- [隐私标记与图汇聚](../../../product/actrium-v1/13-privacy-state-and-aggregation.zh.md)
- [隐私原语与远程证明](../../../product/actrium-v1/14-privacy-primitives-and-attestation.zh.md)

产品稿已按现行模型修订。再出现冲突时，改产品稿，不改本目录。

形式证明与缺陷表见 [formal/model.zh.md](formal/model.zh.md)、[formal/defects.zh.md](formal/defects.zh.md)。机器核验：`python3 docs/architecture/security/actrium/formal/proof/check_proofs.py`。

审查过程（问答卡、决定流水、冲突、TODO、停车场）不在本仓库。本地副本位于被 git 忽略的 `internal/security-review/`。
