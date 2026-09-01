# Actrium 隐私安全形式模型、证明与可行性验证

> 状态：`Machine-checked specification theory + bounded STS`  
> 对照基线：[语义模型](../02-model.zh.md)、[协议与执行](../03-protocol.zh.md)、[产品标记](../04-product.zh.md)  
> 机器核验：[proof/check_proofs.py](proof/check_proofs.py)、[check_formal_model.py](check_formal_model.py)  
> 缺陷表：[defects.zh.md](defects.zh.md)  
> 目的：在显式公理下给出可核验推导；有限载体完全判定；有界迁移系统穷举安全不变量。  
> 非目标：不证明现实 Host／TEE／密码实现，也不把 `Specified` 提升为 `Deployed`。  
> 读者摘要见 [保障、证明与成熟度](../05-assurance.zh.md)。

记号：`Def` 是定义，`Lem` 是直接推论，`Thm` 是需要论证的命题，`Prop` 依赖密码或执行假设，`Imp` 是不可能性。

## 0. 结论先行

在本文明确列出的假设下：

1. **逻辑自洽**（Thm 1）。存在满足全部语义约束的绿、黄、红实例。
2. **相对可靠**（Thm 7）。若 A1–A13 成立，且快照当前、图就绪，绿色足以推出：当前不存在可执行的模型内泄漏。绿色只保证这次就绪切面，不保证曾经存在一个全图同时世界，也不否定历史上已经发生的泄漏。
3. **弱项不可抵消**（Thm 3–6、A13）。未受控必红；无红而有未知必黄。增加节点、风险接受、数据输入授权都不能把弱项洗绿。同一评定上下文的多份结论取最弱项。
4. **图外 Actor 出口不能靠纳图偷绿**（Thm 8）。跨 Principal 或无效依赖的 `Transfer(Actor(_))` 不入图，且不得判 `CONTROLLED`。
5. **状态、世代与执行隔离**（Thm 10–14、18）。`ActorStateId` 变化后，旧通道、旧 Capability、旧结论失效；单调修订号保证内容回滚也不复活旧权力；挂起期间不得产生新的应用 `Transfer`／`Retain`；旧数据不能走新出口；信箱按当前声明与取得世代声明的交集执行。
6. **发布抗拼接**（Thm 9、15、19）。混合快照不能成为可接受的 `ApplicationAssurance`；叶子必须同时绑定 Actor、状态和节点结论；`EncGraph` 对带标签图单射。
7. **不需要全图分布式提交**（Thm 12 推论、Imp 1、Thm 20）。本地原子废除旧权力即可保证安全；未依赖变化节点的成员可以局部继续。
8. **失联与撤销有可证上界**（Imp 2–3、Thm 16、21）。最高保障必须本地收口；期限相对本地不可回拨时钟。完全离线最迟到 `expiresAt`。授权不可自行扩大见 Thm 17。
9. **可计算**（§19）。有限快照下组图、评定、Merkle 证明均可终止；主体线性，成员证明对数。
10. **工程前提不会被公式掩盖**。A2、A4、A5、A6、A8、A9 证明不了，就必须保持黄／红或拒绝建立结论。

本文证明的是规范层**相对可靠性**：颜色不会比已建模事实更强。它不是无条件的「系统已安全」。

## 1. 证明层级与范围

形式化证明必须写成：

$$
\text{Assumptions} \land \text{Protocol Invariants}
\Longrightarrow
\text{Privacy Claim}
$$

不能写成「设计文档存在 \(\Longrightarrow\) 真实系统安全」。

当前分层：

| 层 | 内容 | 状态 |
| --- | --- | --- |
| 共享守卫 | Exec／时钟／世代／发布／Ready 的守卫单一定义源，STS 与 Z3 共用 | [proof/semantics.py](proof/semantics.py) |
| 机械化 | 三值格、图划分、Merkle、判断、跨 Principal、颜色独立、绿色相对安全 | [proof/theory.py](proof/theory.py) 有限完全判定；[proof/z3_theory.py](proof/z3_theory.py) SMT 量词证明（守卫取自共享定义；部分条目为定义一致性锁定，非独立证明） |
| 有界 STS | Change／Attest／Resume／Publish；n≤2 可达状态；另含数据世代、对抗性本地时钟、回滚复活专用穷举 | [proof/sts.py](proof/sts.py)，n=2 时 14,400 态；旧快照不被全局抹除，由 Ready 指针核对拒绝；触及状态上限即失败 |
| 握手穷举 | 联合 transcript 认证下的身份／状态绑定、重放与两侧一致性（双侧独立视图） | [proof/channel.py](proof/channel.py)，7,056 条迹 |
| 回归 | 对象绑定、证据、Capability、挂起、回滚、编码字符集 | [check_formal_model.py](check_formal_model.py) |
| 变异回归 | 把守卫换成已知坏定义，断言对应检查器必须报警（防「守卫即模型」空洞） | [proof/mutants.py](proof/mutants.py)，8 个变异体 |
| 公理 | 完整中介、计算密码学、证据覆盖现实、可信时间、完整发现 | A1–A13，见缺陷表 |
| 未做 | Lean／Tamarin 无界归纳与计算密码归约；实现符合性 | §22 |

本文不证明：某个 Host 没有漏洞；某家 TEE 根永远可信；哈希或 AEAD 在现实中绝不失效；用户信任前提为真；通信元数据不泄露；拒绝服务和侧信道被消除。

## 2. 数学对象

### 2.1 基本集合

- \(\mathcal A\)：可识别 Actor 实例；
- \(\mathcal P\)：PrincipalId；
- \(\mathcal X\)：非 Actor 目标，例如屏幕、文件、Workspace 资源、网络地址；
- \(\mathcal S_a\)：Actor \(a\) 的可能状态；
- \(\mathcal E\)：证据对象；
- \(\mathcal Q\)：用户信任策略；
- \(\mathcal W\)：风险接受记录；
- \(\mathcal I\)：数据输入授权。

$$
id:\mathcal A\to ActorId
\qquad
principal:\mathcal A\to\mathcal P
$$

Principal 绑定在实例生命周期内不变。

### 2.2 Actor 当前状态

$$
s_a(t)\in\mathcal S_a
$$

$$
sid_a(t)=H(
  \mathsf{actrium/actor\text{-}state/v1},
  id(a),
  principal(a),
  rev_a(t),
  h_{pkg},h_{cfg},h_{cap},h_{env}
)
$$

四个子指纹分别是软件包、实例配置、有效能力声明和宿主环境。\(rev_a\) 是状态权威维护的单调递增修订号：每次 Change 严格加一，永不回退。因此**内容回滚不复现旧 \(sid\)**——把配置改回旧值得到的是携带新修订号的新状态，绑定旧 \(sid\) 的 Capability、通道与 `ActorAssurance` 不会复活（Thm 10）。证据、策略和评定结果不属于 \(s_a\)。

### 2.3 隐私效果

$$
T(a,s)\subseteq
\{\operatorname{Transfer}(Actor(b))\mid b\in\mathcal A\}
\cup
\{\operatorname{Transfer}(x)\mid x\in\mathcal X\}
$$

现实保留事实：

$$
r(a,s)\in\{0,1\}
$$

验证者对它的认知：

$$
\widehat\rho_{\Gamma,q}(a,s)\in\{0,?,1\}
$$

- \(0\)：\(\Gamma,q\vdash r(a,s)=0\)；
- \(?\)：无法确认；
- \(1\)：\(\Gamma,q\vdash r(a,s)=1\)。

`Retain` 不按时长分档。

### 2.4 应用数据与世代

已进入 Actor \(a\) 的应用数据对象 \(d\) 带有取得世代：

$$
\operatorname{gen}(d)\in\mathcal S_a
\qquad
\operatorname{sid}(\operatorname{gen}(d))=sid_a(t_{\mathrm{acq}}(d))
$$

对新产生或新接收的数据，\(\operatorname{gen}(d)\) 等于当前状态。对旧数据：

$$
\operatorname{Allowed}(a,d)=T\bigl(a,s_a(t)\bigr)\cap T\bigl(a,\operatorname{gen}(d)\bigr)
$$

实现可以把该交集戳在数据对象上，但必须与取得时声明一致。

### 2.5 三值事实域

$$
\mathbb L=\{\mathbf C,\mathbf U,\mathbf X\}
\qquad
\mathbf C\prec\mathbf U\prec\mathbf X
\qquad
\sqcup=\max
$$

该序只表示顶层显示谁优先，不是可加的风险距离。空集上确界为 \(\mathbf C\)。

$$
v_R(\widehat\rho)=
\begin{cases}
\mathbf C,&\widehat\rho=0\\
\mathbf U,&\widehat\rho=?\\
\mathbf X,&\widehat\rho=1
\end{cases}
$$

## 3. 判断关系

\(\vdash\) 此前是原语。本节给出最小推导规则。它仍是抽象评定，不是某家 TEE 格式的解析器。

### 3.1 可接受证据

证据项：

$$
evi=(issuer,authentic,binding,notAfter,claim,payload)
$$

其中 \(binding=(actor,sid)\)。

**E-Acc**  
若 \(authentic(evi)\)、\(issuer\in TrustedIssuers(q)\)、\(issuer\neq id(a)\)、\(binding=(id(a),sid_a)\)、\(now_v<notAfter\)，则

$$
evi\in Acc(\Gamma,q,a,s)
$$

\(now_v\) 是评定方（验证者或依赖方）的本地时钟，不是被评估 Actor 的 \(now_a\)。签名失败、过期、绑错状态、自签或签发者不被 \(q\) 信任，都不能进入 \(Acc\)。未进入 \(Acc\) 的材料不能推出 \(\mathbf C\) 或 \(\mathbf X\)。

### 3.2 Transfer

记 \(e=\operatorname{Transfer}(\tau)\)。

**E-X-OutGraph**  
若 \(\tau=Actor(b)\) 且 \(\neg ValidDependency(a,b)\)，则

$$
\Gamma,q\nvdash Controlled(a,e)
$$

若该效果已在有效声明中被确认为可能（A2），则

$$
\Gamma,q\vdash Uncontrolled(a,e)
$$

目标无法解析时，停在 \(\mathbf U\)，不得判 \(\mathbf C\)。

**E-C-Actor**  
若 \(\tau=Actor(b)\)、\(ValidDependency(a,b)\)、通道满足 A5，且 Host 不能把该效果的明文送到 \(b\) 以外，则

$$
\Gamma,q\vdash Controlled(a,e)
$$

这只说明「明文端点是 \(b\)」，不说明 \(b\) 自己安全。\(b\) 的出口另行评定。

**E-C-Sink**  
若 \(\tau\in\mathcal X\)、Host 把实际目标不可绕过地限制在集合 \(X\)，且 \(X\subseteq TrustedSinks(q)\)，并有 \(Acc\) 中相应 \(restricts\) 证据，则

$$
\Gamma,q\vdash Controlled(a,e)
$$

精确地址不属于 \(TrustedSinks(q)\) 时，不得仅因「写得很准」判 \(\mathbf C\)。

目标分类本身是被约束的事实，不是声明方的自由选择：凡是能出示 Actr 身份、可按 Actor 评定的端点，必须按 \(\operatorname{Transfer}(Actor(b))\) 处理，不得改写为 \(\mathcal X\) 中的域名或 API 目标来绕过跨 Principal 规则（E-X-OutGraph）。Host 或验证者无法判定目标类别时，停在 \(\mathbf U\)。

**E-X-Transfer**  
若 \(Acc\) 中存在确认该出口已超出可信行为边界的证据，则

$$
\Gamma,q\vdash Uncontrolled(a,e)
$$

**E-U**  
否则

$$
v_{\Gamma,q}(a,e)=\mathbf U
$$

### 3.3 Retain

**E-R0**  
\(Acc\) 中存在 \(unrecoverable\) 主张，且其覆盖全部恢复路径（内存外读、快照、备份、日志、密钥托管），则 \(\widehat\rho=0\)。

**E-R1**  
\(Acc\) 中存在 \(recoverable\) 主张，则 \(\widehat\rho=1\)。

**E-R?**  
否则 \(\widehat\rho=?\)。覆盖不全的「密钥已删除」不得推出 \(\widehat\rho=0\)。

### 3.4 合成

$$
v_{\Gamma,q}(a,e)=
\begin{cases}
\mathbf X,&\Gamma,q\vdash Uncontrolled(a,e)\\
\mathbf C,&\neg(\Gamma,q\vdash Uncontrolled(a,e))
  \land\Gamma,q\vdash Controlled(a,e)\\
\mathbf U,&\text{otherwise}
\end{cases}
$$

同时出现受控与未受控依据时，\(\mathbf X\) 优先，两类原因都保留。判断关系非爆炸：假证据或互相冲突的不可信材料不能推出任意结论。

$$
R_A(a,s,\Gamma,q)
=
\Bigl(\bigsqcup_{e\in T(a,s)}v_{\Gamma,q}(a,e)\Bigr)
\sqcup
v_R(\widehat\rho_{\Gamma,q}(a,s))
$$

**Lem 0　无证不得强判**  
若 \(Acc(\Gamma,q,a,s)=\emptyset\) 且 \(T(a,s)\neq\emptyset\)，则 \(R_A\neq\mathbf C\)。

### 3.5 多份 ActorAssurance

§3.4 给出**单份**评定的 \(R_A^{AA}\)。记评定上下文 \(ctx=(sid_a,qid,ruleset)\)。

$$
AccAA(ctx)=\bigl\{AA\mid AA\text{ 通过签名、策略、有效期和 A11，且绑定 }ctx\bigr\}
$$

**E-Join-AA**　节点实际采用的结论是

$$
R_A^{\mathrm{used}}(a,s,\Gamma,q)
=
\begin{cases}
\mathbf U,&AccAA(ctx)=\emptyset\land T(a,s)\neq\emptyset\\
\mathbf C,&AccAA(ctx)=\emptyset\land T(a,s)=\emptyset\\
\bigsqcup\bigl\{R_A^{AA}\mid AA\in AccAA(ctx)\bigr\},&AccAA(ctx)\neq\emptyset
\end{cases}
$$

格的空集上确界是 \(\mathbf C\)，但不能把「没有被接受的 AA」写成绿：有出口却无结论时记 \(\mathbf U\)。Publish 快照里那一份的 \(R_A^{AA}\) 不得强于 \(R_A^{\mathrm{used}}\)。禁止从 \(AccAA\) 中挑选较绿的子集。后文 \(R_A(a)\) 均指 \(R_A^{\mathrm{used}}\)。

不同 \(qid\) 或规则集是不同评定，不是冲突。单一被信任验证者作假，是 \(q\) 为假（A6），不是两家打架。

\(AccAA(ctx)\) 的最弱项约束作用于评定者**可见**的被接受集合。被扣留、未送达的更弱结论不可检测；本模型不声称能发现「存在但从未出示的 AA」。

## 4. 应用图

### 4.1 有效依赖

$$
ValidDependency(a,b)
\iff
\operatorname{Transfer}(Actor(b))\in T(a,s_a)
\land resolvable(b)
\land principal(a)=principal(b)
\land current(s_a)
$$

$$
D_{\vec s}
=
\{(a,b)\mid ValidDependency(a,b)\}
$$

能力存在只形成逻辑依赖，不产生调用授权。

### 4.2 无根图

$$
Applications(\vec s)=CC(U(D_{\vec s}))
$$

\(U\) 忽略边方向，\(CC\) 取连通分量。孤立 Actor 是单节点图。当前连接、通信和工作状态不出现在定义中。

\(EncGraph(G)\) 是如下 UTF-8 JSON，无空白，数组顺序固定：

$$
\begin{aligned}
EncGraph(G)=\;&\mathsf{JSON}\bigl[
  \mathsf{actrium/graph/v1},\;
  1,\;
  principal(G),\\
  &\quad \mathrm{sort}(\{id(a)\mid a\in V_G\}),\;
  \mathrm{sort}\bigl(\{(id(a),id(b))\mid (a,b)\in D_G\}\bigr)
\bigr]
\end{aligned}
$$

$$
structureId(G)=H(\mathsf{actrium/structure/v1},EncGraph(G))
$$

节点集、边集或 Principal 任一不同，则编码字节不同（Thm 19）。输入顺序、空白或对象键序不得改变摘要。依赖增删会改变 \(structureId\)，除非发生哈希碰撞（A4）。

进入 \(EncGraph\) 的 `ActorId` 与 `PrincipalId` 必须落在受限 ASCII 字符集 `[a-z0-9._:-]`（推荐：指纹的小写十六进制），超出即拒绝编码。这消除 JSON 字符串转义与 Unicode 归一化在跨语言实现间的歧义空间。

## 5. 四对象与 Merkle 树

### 5.1 ActorAssurance

$$
AA_a=Sign_v(
  sid_a,H(\Gamma_a),qid,ruleset,validity,findings,R_A
)
$$

它不授予权限，也不绑定接收者。

### 5.2 叶子与树

$$
leaf_a=H(\mathsf{actrium/application\text{-}leaf/v1},id(a),sid_a,H(AA_a))
$$

对按 ActorId 排序后的叶子序列 \(L=(leaf_1,\dots,leaf_N)\) 构造 RFC 6962 风格二叉树：

$$
h^{leaf}(x)=H(\mathsf{actrium/merkle\text{-}leaf/v1},x)
$$

$$
h^{node}(\ell,r)=H(\mathsf{actrium/merkle\text{-}node/v1},\ell,r)
$$

奇数层把最后一个节点直接提升，不复制叶子。空树摘要为 \(H(\mathsf{actrium/merkle\text{-}empty/v1})\)。

$$
appid=H(
  \mathsf{actrium/application\text{-}state/v1},
  structureId(G),
  principal(G),
  MerkleRoot(L)
)
$$

成员证明是从叶子到根的兄弟哈希路径，长度 \(O(\log N)\)。根承诺的是排序后的叶子多重集合；交换两个不同叶子会改变根。

### 5.3 ApplicationAssurance

$$
APPA=Sign_z(appid,qid,ruleset,validity,R_G,reasons)
$$

$$
R_G=\bigsqcup_{a\in V_G}R_A(a)
$$

$$
color(R_G)=
\begin{cases}
Green,&R_G=\mathbf C\\
Yellow,&R_G=\mathbf U\\
Red,&R_G=\mathbf X
\end{cases}
$$

该函数只对结构合法、必要结论可验证、通道满足 A5 的应用状态定义。否则不产生第四色，而是没有可用 `ApplicationAssurance`，并阻断相应通信。

## 6. 核心假设

这些不是免费事实，而是定理成立所需的显式前提。

### A1　有限快照

每次评定的 \(V_G\)、\(D_G\) 和每个 \(T(a,s)\) 均有限可枚举。

### A2　完整中介与能力闭合

任何可能使边界外主体取得应用信息的动作，都经过不可绕过执行点；有效能力声明包含全部不能被该执行点排除的 `Transfer` 和 `Retain`。

### A3　状态权威唯一且原子

每个 Actor 在一个修订期内只有一个可认证状态权威。它可以原子切换当前 `ActorStateId`，并同步让旧状态的密钥、通道和受保护动作失效。

### A4　内容寻址和签名安全

规范编码无歧义；哈希满足目标抗碰撞和二次原像抗性；签名不可伪造；密钥不从实现旁路泄漏。

### A5　有效 Actr 通道

Actor-to-Actor 应用数据仅通过满足端到端机密性、完整性、双方 `ActorId + ActorStateId` 绑定和重放防护的通道传输。

### A6　证据可靠性

\(Acc\) 中的证据准确绑定当前实例、配置、环境和有效期；不得把未覆盖的事实解释成已证明。若 \(\mathbf C\) 依赖用户对主体或行为承诺的信任，相对安全定理把该承诺为真当作显式前提。

### A7　策略确定性

相同事实、证据、规则和策略版本给出确定结果。风险接受和数据输入授权不进入颜色函数。

### A8　可信本地时间与收口

需要失联后继续计算的 Actor 具有 Workload 不可延长、不可通过时钟回拨推迟的最晚使用期限；到期后明文能力和效果由本地可信机制关闭。

不可回拨要求同样适用于**一切参与接受判定的时钟**：E-Acc 与 A10 里评定方核对 \(notAfter\) 所用的 \(now_v\) 若可被本地攻击者回拨，过期证据与过期结论会被重新接受。

### A9　完整图发现

发现机制取得无向依赖的完整连通分量，包括只通过入边与种子相连的 Actor。

### A10　依赖方复验

持有 `ApplicationAssurance` 不等于可以跳过核验。使用方必须检查签名、验证者当前信任、每个相关 `ActorStateId` 是否仍为当前值，以及用**本方**本地时钟核对 `not_after`。

### A11　验证者不是被评估者

\(issuer\neq id(a)\)。被评估 Actor 不能自签最终结论。

### A12　展示就绪

对外显示颜色当且仅当

$$
Ready(\sigma)
=
App\neq\bot
\land Current(snapshot)
\land \forall a\in V_G.\; gate_a=ACTIVE
$$

任一 Actor 仍挂起时，不得把旧或新的绿色当作当前展示。

### A13　同一评定上下文取最弱项

节点结论是 \(R_A^{\mathrm{used}}\)（E-Join-AA）。实现不得在 \(AccAA(ctx)\) 中挑选较强结论，也不得把空的 \(AccAA\) 当成 \(\mathbf C\)（有出口时）。因此同一 \(ctx\) 下不存在「两家打架、系统选绿」的合法状态。

## 7. 迁移系统

系统配置：

$$
\sigma=
\bigl(
\{(a,s_a,sid_a,gate_a,K_a,Ch_a,MB_a,T^{max}_a,now_a)\},
App
\bigr)
$$

\(now_a\) 是 \(a\) 的本地时钟，只增不减。各 Actor 的 \(now_a\) 不必相等。

\(gate_a\in\{ACTIVE,SUSPENDED\}\)。  
\(App=\bot\) 或 \((G,appid,APPA,snapshot)\)。

受保护操作 \(Protected=\{Transfer,Retain,ProcessAppData\}\)；管理操作 \(Admin=\{attest,exit\}\)。

### 7.1 Change

$$
Change_a(s\to s'):
\begin{cases}
rev_a:=rev_a+1\\
sid_a:=sid(s')\\
gate_a:=SUSPENDED\\
K_a:=\emptyset,\quad Ch_a:=\emptyset\\
disable(ProtectedEffects(s))\\
emit(StateChanged(a,sid(s),sid(s')))
\end{cases}
$$

信箱中已有数据的世代不变，不得改写成 \(s'\)。前五项属于同一本地提交边界。事件可以延迟或丢失。修订号只增不减，因此 \(sid(s')\) 必然是新值，即使 \(s'\) 的内容与某个历史状态相同。已发布的旧 `ApplicationAssurance` 副本**不会被全局抹除**——它们由依赖方按 A10 对当前指针复验时拒绝，不依赖任何全图原子删除。

### 7.2 Exec

前置：

1. 若 \(op\in Protected\)，则 \(gate_a=ACTIVE\)、\(now_a<T^{max}_a\)、当前密钥属于 \(sid_a\)；
2. 若 \(op\in Protected\)，则 Capability 通过 §18 的验证（时间窗口用 \(now_a\)）；
3. 若 \(op\) 作用于数据 \(d\)，则效果属于 \(\operatorname{Allowed}(a,d)\)；新产生或新接收的数据 \(\operatorname{gen}(d)=s_a\)，因而退化为当前声明；
4. 若 \(op=Transfer(Actor(b))\)，则 \(gate_b=ACTIVE\) 且 \(ValidDependency(a,b)\)；
5. 若存在 \(c\) 使 \(gate_c=SUSPENDED\) 且 \(\operatorname{Depends}(a,c)\)，则禁止 \(Protected\)；
6. 不得把数据 \(d\) 的世代改写成当前状态，除非 \(d\) 是此刻新产生或新接收的。

挂起期间只允许 \(Admin\)。不得处理应用数据。

### 7.3 Resume

前置：\(gate_a=SUSPENDED\)，且存在绑定当前 \(sid_a\) 的有效 `ActorAssurance`，且验证者满足 A11。成功后 \(gate_a:=ACTIVE\)，并为当前状态恢复密钥。若新应用颜色相对旧结论变弱，相关工作仍保持阻断，直到风险接受。

### 7.4 Publish

前置：

1. \(G=CC(U(D_{snapshot}))\) 且满足 A9；
2. 每个叶子的 \(leaf.sid=currentSid_a=AA_a.sid\)；
3. 每个 \(AA_a\) 通过签名、策略、有效期和 A11 检查；其 \(R_A^{AA}\) 不得强于 \(R_A^{\mathrm{used}}\)；
4. 每个 \(a\in V_G\) 的 \(gate_a=ACTIVE\)；
5. \(appid\) 按 §5.2 重算一致。
6. 若某节点 \(AccAA=\emptyset\) 且 \(T\neq\emptyset\)，不得把它当作 \(\mathbf C\) 发布。
7. 每个 `ActorId` 在叶子序列中恰好出现一次。

任一混合或仍有挂起即失败，不得发布。成功后 \(App:=(G,appid,APPA,snapshot)\)。

该快照是发布核对时刻对当前指针的读一致切面，不是全部 Actor 内部状态的同时物理切面。Thm 7 的绿色安全相对于 \(Ready(\sigma)\)，不是相对于一份脱离当前指针的历史 `APPA`，也不相对于「曾经存在那样一个同时世界」。

### 7.5 Depends 与 LocalContinue

$$
\operatorname{Depends}(b,a)
\iff
(a,b)\in U(D_{\vec s})
$$

即无向有效依赖边。无法判定时视为依赖。

$$
\begin{aligned}
LocalContinue(b\mid C)
\iff\;&
gate_b=ACTIVE\\
&\land\;AA_b\text{ 绑定当前 }sid_b\text{ 且仍适用}\\
&\land\;\forall a\in C.\;\neg\operatorname{Depends}(b,a)\\
&\land\;\neg DisplayGreen
\end{aligned}
$$

满足时 \(b\) 可以继续不涉及 \(C\) 的自身 `Protected` 工作。不满足则必须停相应应用工作。

### 7.6 本地时钟

$$
Tick_a:\quad now_a:=now_a+\delta\quad(\delta>0)
$$

禁止减小 \(now_a\)。没有全局 \(now\)。Capability 与 \(T^{max}\) 只和执行方的 \(now_a\) 比较。

## 8. 自洽性

### Thm 1　模型非空且三色均可达

**构造：**

1. \(M_G\)：单 Actor，受约束 Wasm/Keel，\(T=\emptyset\)，\(r=0\) 且 \(\widehat\rho=0\)。则 \(R_A=\mathbf C\)。
2. \(M_Y\)：单 Native Actor，Host 无法排除一条网络旁路，也无证据确认旁路可用。该效果为 \(\mathbf U\)。
3. \(M_R\)：已确认边界后仍可恢复日志，\(\widehat\rho=1\)，故 \(v_R=\mathbf X\)。

三者都满足唯一 Actor 类型、单 Principal、无根图和三值汇聚。公理组不是矛盾系统，也不是只能产出一种颜色的退化系统。

### Thm 2　应用图边界唯一

对有限 \(D\)，\(CC(U(D))\) 是节点集的唯一划分，与种子和顺序无关。

**证明：** 「无向可达」是等价关系。等价类唯一。DFS／BFS／并查集只是算法。证毕。

若实现只沿出边走，A9 不成立，得到的不是该定义下的应用图。

## 9. 聚合性质

### Lem 1　\(\sqcup\) 的交换、结合与幂等

\(\sqcup=\max\) 在全序 \(\mathbb L\) 上直接成立。因此 Actor 和应用结果与遍历顺序、并发完成顺序和重复缓存无关。

### Thm 3　弱项不可抵消

若存在 \(a\in V_G\) 使 \(R_A(a)=\mathbf X\)，则 \(R_G=\mathbf X\)；若无 \(\mathbf X\) 但存在 \(\mathbf U\)，则 \(R_G=\mathbf U\)。

**证明：** \(R_G\) 是最大值。证毕。

### Thm 4　增加必要 Actor 不能让结果变强

$$
R_{G'}=R_G\sqcup R_A(b)\succeq R_G
$$

绿色可以保持、变黄或变红，不能由黄／红变绿。

### Thm 5　风险接受不影响颜色

\(R_G\) 的定义域是 \((\vec s,\Gamma,q)\)，不含 \(w\in\mathcal W\)。故

$$
R_G(\vec s,\Gamma,q,w_1)=R_G(\vec s,\Gamma,q,w_2)
$$

$$
RunAllowed
=
(color\neq\bot)
\land
\bigl((R_G=\mathbf C)\lor Accepted(w,appid,reasons)\bigr)
$$

\(color=\bot\)（图或通道无效）时，\(w\) 不能放行。

### Thm 6　数据输入授权不影响颜色

\(T(a,s)\) 与 \(\widehat\rho\) 不由 \(i\in\mathcal I\) 裁剪。状态、证据和策略不变时，\(R_G(i_1)=R_G(i_2)\)。

Thm 5、Thm 6 是定义约束，不是深定理；写成定理是为了禁止实现把 \(w\) 或 \(i\) 偷偷送进颜色函数。回归脚本把二者作为显式参数并断言输出不变。

## 10. 绿色相对可靠性

### Def　模型内隐私泄漏

发生模型内泄漏，当且仅当：

1. 应用信息被当前 Actor 边界外、且不属于 \(TrustedSinks(q)\) 的明文目标取得；或
2. 预定临时使用边界结束后，Actor 或 Host 仍能恢复应用信息。

通信元数据不是模型内泄漏。

### Lem 2　两效果覆盖模型内泄漏

在 A2 下，任何模型内泄漏对应一项未受控 `Transfer` 或 \(r=1\)。没有第三种模型内泄漏。

这是相对 A2 的定义完备。A2 不成立时，旁路不进入 \(T(a,s)\)，形式模型看不见它。

### Thm 7　绿色安全

设 \(Ready(\sigma)\) 且展示颜色为绿。在 A1–A13 下，**当前就绪切面**上不存在可执行的模型内隐私泄漏。

绿色不蕴含「历史上从未泄漏」，也不蕴含「所有 Actor 内部状态曾在某一物理瞬间同时等于该切面」。能力被收窄并重新评定后，旧泄漏仍是历史事实，新状态可以变绿。旧数据仍受 Thm 18 约束。

**证明：**

1. \(Ready\) 给出当前快照、全部 \(gate=ACTIVE\)，且 \(color\neq\bot\)。
2. 绿要求对所有 \(a\in V_G\)，\(R_A(a)=\mathbf C\)。
3. 故每个 \(e\in T(a,s)\) 均为 \(\mathbf C\)，且 \(\widehat\rho(a)=0\)。
4. 由 E-X-OutGraph，\(T(a,s)\) 中不存在无效依赖的 Actor 目标。
5. 对 \(Transfer(Actor(b))\)，有 \(ValidDependency(a,b)\) 且 \(gate_b=ACTIVE\)。A5 保护该边。\(b\) 自身 \(R_A(b)=\mathbf C\)。
6. 对非 Actor 目标，E-C-Sink 给出 Host 强制且 \(X\subseteq TrustedSinks(q)\)。
7. A6 与 E-R0 给出 \(r=0\)。
8. Exec 不得执行未声明效果（A2）。因此当前可执行效果中没有模型内泄漏。证毕。

A6 把「用户信错主体」保留为相对前提：\(Green=Green(\vec s,\Gamma,q)\)。\(q\) 为假时，形式推导不把现实一起变绿。

### 推论　一处弱项足以否定绿色

任一未受控 Transfer 或 \(\widehat\rho=1\) 使应用为红；任一无法确认的必要事实使应用最多为黄。

## 11. 图外 Actor Transfer

### Thm 8　无效 Actor 依赖不能判受控

设 \(e=Transfer(Actor(b))\in T(a,s)\) 且 \(\neg ValidDependency(a,b)\)。则：

1. \((a,b)\notin D_{\vec s}\)，因此 \(b\notin V_G\)（除非经其他有效路径相连；跨 Principal 时通常不相连）；
2. \(\Gamma,q\nvdash Controlled(a,e)\)；
3. 若 A2 确认该效果可能被行使，则 \(v(a,e)=\mathbf X\)；
4. 因而 \(R_A(a)\neq\mathbf C\)，应用不能为绿。

**证明：** 由 E-X-OutGraph 与 Thm 3。证毕。

这关掉「把数据送给另一 Principal 的 Actor，却因对方不入图而不评分」的漏洞。对方不入图，出口仍在本节点计分。

## 12. 抗拼接

### Lem 3　Actor 状态敏感

任一固定角色组成不同，且编码无歧义时，\(sid(s)=sid(s')\) 只可能来自状态相同或哈希碰撞。A4 下后者计算上不可行。

### Lem 4　成员绑定

给定 `ApplicationAssurance + ActorId + ActorStateId + ActorAssurance + MerkleProof`，验证者能确认该叶子被 `appid` 承诺。替换任一项要求伪造签名或找到碰撞。

**证明：** 叶子绑定三项；路径绑定叶子到 `MerkleRoot`；`appid` 再绑定结构、Principal 和该根；`APPA` 签在 `appid` 上。证毕。

### Thm 9　四对象链抗跨状态拼接

攻击者不能把 A 的状态、B 的节点结论、旧策略证明和新结构签名拼成诚实依赖方会接受的当前 `ApplicationAssurance`。

**证明：** 任一替换改变叶子、`appid` 或签名消息。A10 要求逐项检查身份、状态、策略、规则、有效期和签名。证毕。

## 13. 状态变化、挂起与信箱

### Thm 10　旧 Capability 自动不匹配

Capability 绑定 \((ActorId,sid(s))\)。变化后 \(sid(s')\neq sid(s)\)，相等检查失败。

因为修订号 \(rev_a\) 参与 \(sid\) 且单调递增，本定理**不依赖「状态不回访」**：把配置内容改回旧值得到的仍是新 \(sid\)，历史上签发的 Capability 不因内容回滚而复活（回滚穷举见 [proof/sts.py](proof/sts.py) 的 rollback 模型，回归见 `verify_rollback_revision`）。撤销语义因此是持久的：一旦 Change，绑定旧 \(sid\) 的权力在该实例的余下生命周期内永久不匹配，除非签发者对新状态另行签发。

### Thm 11　旧通道不能承载新状态应用数据

**Prop**（A3 ∧ A5）。握手绑定双方当前 `ActorStateId`。状态权威废除旧会话密钥后，旧通道不能被当前状态合法使用。

只把状态 ID 写进握手、却允许 Workload 继续使用旧私钥，不足以证明本定理。

### Thm 12　旧应用结论失去适用性

\(a\) 的状态改变会改变 \(leaf_a\) 和 \(appid\)。旧 `APPA` 签在旧 `appid` 上。证据或策略变化而 `ActorState` 不变时，\(H(AA_a)\) 仍会改变叶子。

### 推论　不需要跨 Actor 分布式原子提交

变化 Actor 在本地提交时立即拒绝旧权力，其他 Actor 即使没收到事件，也不能通过它执行旧受保护效果。事件和重算恢复可用性，不是本地安全的前提。

### Thm 13　挂起隔离

\(Change_a\) 之后、\(gate_a\) 仍为 `SUSPENDED` 时：

$$
\neg Exec(a,Transfer)
\land
\neg Exec(a,Retain)
$$

即使攻击者持有绑定新 \(sid_a\) 的 Capability，也不得在挂起期间对应用数据产生这两类效果。`attest`／`exit` 可以发生。

**证明：** 由 Exec 前置 1。证毕。

实现若不能隔离，必须停止相应应用工作；此时模型按「无法隔离则视为仍可能 Exec」处理，不能给绿。

### Thm 14　信箱按当前声明与数据世代的交集执行

消息不携带可执行的发送时权限快照。对已持有数据 \(d\)：

$$
\operatorname{Allowed}(a,d)=T(a,s_{\mathrm{now}})\cap T(a,\operatorname{gen}(d))
$$

新产生或新接收的数据 \(\operatorname{gen}(d)=s_{\mathrm{now}}\)，交集退化为当前声明。\(Exec\) 还检查当前 \(sid_a\)、当前 Capability、当前 \(gate_a\)、本地 \(now_a<T^{max}_a\)，以及依赖方是否挂起。

因此：旧消息不能用已删除的旧出口（当前声明已去掉）；旧数据也不能用状态变化后新出现的出口（Thm 18）；挂起期间仍失败（Thm 13）；重试必须重新走 Exec。

## 14. 快照发布

### Thm 15　混合快照不可发布

若存在叶子使

$$
leaf.sid\neq currentSid_a
\quad\text{或}\quad
AA_a.sid\neq leaf.sid
$$

则 Publish 失败。

**证明：** 由 §7.4 前置 2。证毕。

### 讨论　读一致切面不是全局同时切面

Publish 成功只说明：在核对那一次读取中，所有指针与叶子一致。Actor 内部可以在核对前后继续变化。安全性不依赖「这份 \(\vec s\) 曾在某一物理瞬间同时成立」，而依赖：

- 旧权力已按 Thm 10–14 在各自本地失效；
- 依赖方按 A10 在使用时再次核对此刻的当前指针。

持续抖动可以使候选反复作废。这是活性失败：需要整图保障的操作保持阻断，旧状态不会因此恢复权力。

### Thm 18　新配置只作用于新数据

设 \(Change_a\) 把声明从 \(T\) 改为 \(T'\)，且 \(e\in T'\setminus T\)。对 \(\operatorname{gen}(d)\) 对应旧状态的数据 \(d\)：

$$
\neg Exec\bigl(a,\operatorname{Transfer}_e(d)\bigr)
$$

即使 Resume 之后 \(gate_a=ACTIVE\) 且存在绑定新 \(sid_a\) 的 Capability。

**证明：** Exec 前置 3 要求 \(e\in T\cap T'\)。\(e\notin T\)。证毕。

处理旧数据也不得把它的世代改写成新状态。新接收或新产生的数据使用 \(T'\)。

### Thm 19　\(EncGraph\) 对带标签图单射

设 \(G,G'\) 的 Principal、节点 `ActorId` 集合或有向有效依赖边集合任一不同。则 \(EncGraph(G)\neq EncGraph(G')\)。因而在 A4 下 \(structureId(G)=structureId(G')\) 只可能来自哈希碰撞。

**证明：** 编码是排序后的节点表与边表的规范 JSON。集合不同则序列不同，UTF-8 字节不同。证毕。

输入排列不改变编码：排序消去顺序。

### Thm 20　局部继续不恢复整图绿色

设 \(C\) 为当前挂起成员。若 \(LocalContinue(b\mid C)\)，则 \(b\) 可以执行不涉及 \(C\) 的自身 `Protected` 操作。此时 \(\neg Ready(\sigma)\)，因而不得展示应用绿色（A12）。

若 \(\operatorname{Depends}(b,c)\) 且 \(c\in C\)，则 \(b\) 的 `Protected` 被禁止。无法判定 `Depends` 时按依赖处理。

**证明：** 由 §7.5 与 Exec 前置 5、A12。证毕。

### Thm 21　期限相对本地时钟

\(Exec\) 的时间条件是 \(now_a<T^{max}_a\)，不引用任何全局 \(now\)。因此：

1. \(now_b\) 的快慢不影响 \(a\) 是否过期；
2. 禁止减小 \(now_a\) 时，\(a\) 不能靠拨钟延长自己的期限；
3. 多机偏差可以改变远端何时发现 \(a\) 已过期，但不能延长 \(a\) 本机已到期的权力。

**证明：** 由 Exec 前置 1 与 §7.6。证毕。

没有 A8 的不可回拨本地时钟，本定理对那台机器不成立。

## 15. 不可能性

### Imp 1　异步分区下不能同时保证瞬时全局感知与持续可用

远端 \(r\) 收不到消息时，无法区分「\(a\) 未变、只是延迟」和「\(a\) 已变、事件丢失」。若 \(r\) 在前者为了可用而继续认定旧状态，它在后者也会如此。

因此当前组合是必要的：本地立即废除旧权力；事件加速；有效期和主动复核做最终发现；要求「绝对当前」的操作在分区期间失败关闭。

### Imp 2　无本地可信约束时，远端无法保证失联后清理

Actor 已取得明文后分区。若本地没有不可绕过的期限、密钥门控和效果控制，远端无法区分删除、保留和写入未观察出口。断线后的删除命令可能永远不到。

A8 是失联安全可证成立的必要前提，不是优化。

### Imp 3　完全离线不可能实时感知远端撤销

$$
t_{usable}\le expiresAt
$$

收到可信撤销可以更早失效；完全离线最迟到自身到期。这是可达到的边界。

## 16. 失联安全

数据在 \(t_0\) 交付，本地预先固定 \(T_{max}\)，Workload 不能延长。这里的时间是执行方本地时钟 \(now_a\)。

$$
UseAllowed(a)=(now_a<T^{max}_a)\land KeyUsable(a)\land GateAllows(a)
$$

A8：

$$
now_a\ge T^{max}_a\Longrightarrow\neg KeyUsable(a)\land\neg GateAllows(a)
$$

### Thm 16　期限后的受保护效果不可执行

在 A3、A8 和执行点不可绕过时：

$$
now_a\ge T^{max}_a\Longrightarrow\neg ProtectedEffect(a)
$$

**证明：** 受保护效果发生前必须经过 Gate 并需要相应明文能力。期限后二者皆假。不依赖网络。证毕。

本定理不证明物理介质逐位擦除；它证明在既定威胁模型中，不受信主体已没有可恢复明文的能力。

## 17. 通道证明边界

攻击者：

- 网络：观察密文、延迟、注入、重放、丢弃；
- 信令／TURN：同上，并可改路由和可用性；
- 不能：打破所选 AKE／AEAD，不能取得端点长期密钥或会话密钥。

### Prop 1　明文保密与完整性（A4 ∧ A5）

被动观察者和只转发密文的 TURN 不能取得应用消息明文；篡改密文会被 AEAD 拒绝。

### Prop 2　身份与状态绑定

握手必须认证**联合 transcript**

$$
(id_A,sid_A,ephk_A,id_B,sid_B,ephk_B,session)
$$

而不是只认证单方 Hello。只签本方字段时，旧 `sid` 问候可被信令重放（缺陷 F-CHAN-01）。双方都接受后，各自看到的对端必须是对方当前 `ActorId + ActorStateId`，且两侧的联合 transcript 必须一致。有界 Dolev-Yao 穷举见 [proof/channel.py](proof/channel.py)：A、B 两侧视图独立枚举，攻击者语料含旧会话真实 MAC 与伪造 MAC，「接受即当前、两侧一致」是被检验的结论而非建模前提；单方 MAC 的坏定义由变异回归证明可被检出。该穷举仍是单会话对、符号模型，不替代 Tamarin／ProVerif。

### Prop 3　重放

会话唯一性和序号使旧状态或旧会话的密文不能作为当前应用数据被接受。

### 非性质

信令与 TURN 仍可拒绝服务、观察地址、时序和流量大小。「全过程加密」只表示 Actor 明文端点之间的应用消息内容端到端加密。

上述三条是密码学假设下的协议性质，不是本文内的归约证明。具体套件应交 Tamarin／ProVerif。

## 18. 授权

$$
C=Sign_I(ActorId,ActorStateId,Scope,notBefore,expiresAt,credId)
$$

接受当且仅当：

$$
\begin{aligned}
&Verify_I(C)
\land I\in TrustedIssuers(q)\\
&\land CurrentActor=id(C)
\land CurrentState=sid(C)\\
&\land op\subseteq Scope(C)
\land now_a\in[notBefore,expiresAt)\\
&\land\neg Revoked(C)
\end{aligned}
$$

### Thm 17　Actor 不能自行扩大权力

V1 禁止转授权。不持有签发者密钥的 Actor 不能制造更大 Scope、不同 ActorId、不同 ActorStateId 或更晚有效期的可接受 Capability。复制原凭证不产生新权力，因为使用者身份和当前状态必须精确匹配。

**证明：** 由 A4 与接受谓词。扩大任一绑定字段都会使签名验证失败，或使相等检查失败。证毕。

## 19. 可计算性

记 \(N=|V_G|\)，\(M=|D_G|\)，\(K=\sum_a|T(a,s_a)|\)，\(C_i\) 为第 \(i\) 个 Actor 的证据验证成本。

| 操作 | 时间 | 空间 |
| --- | ---: | ---: |
| 连通分量 | \(O(N+M)\) | \(O(N+M)\) |
| 依赖与 Principal 检查 | \(O(N+M)\) | \(O(N)\) |
| 效果汇聚 | \(O(K)\) | 原因存储量 |
| ActorAssurance 验证 | \(O(\sum C_i)\) | 可并行 |
| Merkle 根 | \(O(N)\) | \(O(N)\) |
| 成员证明 / 单叶更新 | \(O(\log N)\) | \(O(\log N)\) |
| 发布前当前性复核 | 最坏 \(O(N+M)\) | \(O(N)\) |

A1 且结构最终稳定时，算法终止。持续变化不保证产生新 `APPA`（活性），但旧权力不会恢复（安全）。

## 20. 反例回归

| ID | 反例 | 模型如何拒绝 |
| --- | --- | --- |
| CE-01 | 把 Manifest 当有效声明，漏报 Native 网络 | A2 要求补入；不能排除则为黄，确认开放且不可信则为红 |
| CE-02 | 「只保留一分钟」判受控 | \(\widehat\rho=1\mapsto\mathbf X\) |
| CE-03 | 只从 Front 沿出边遍历 | 定义是无向连通分量；出边闭包可以是真子集 |
| CE-04 | 把状态事件当唯一失效机制 | Change 本地废钥；事件可丢 |
| CE-05 | 用户接受后改绿 | Thm 5：\(w\) 不进 \(R_G\) |
| CE-06 | 应用根只承诺 ActorId | 叶子绑三项；换 AA 或 sid 则根变 |
| CE-07 | 无本地期限的远端 LLM | 不满足 A8，不得绿 |
| CE-08 | 图持续变化 | 不保证活性；旧权力不恢复 |
| CE-09 | 跨 Principal Actor 出口不入图、也不评分 | Thm 8：不得 \(\mathbf C\)，确认存在则为 \(\mathbf X\) |
| CE-10 | 挂起后用新 Capability 继续外发旧数据 | Thm 13：挂起禁止 Protected |
| CE-11 | 信箱保存发送时权限 | Thm 14：Exec 看当前 ∩ 世代 |
| CE-12 | 用 A 的当前 sid 配 B 的旧 AA 发布 | Thm 15：混合快照失败 |
| CE-13 | 过期或绑错状态的证据把出口判绿 | E-Acc 拒绝；Lem 0 |
| CE-14 | Resume 后用新出口送旧数据 | Thm 18：\(e\notin T_{\mathrm{gen}}\) |
| CE-15 | 两份 AA 挑较绿的发布 | A13 / E-Join-AA：取最弱项 |
| CE-16 | 依赖已挂起对端仍处理应用数据 | Thm 20：Depends 则禁止 Protected |
| CE-17 | 把对端时钟当成本地 \(T_{max}\) | Thm 21：只看 \(now_a\) |

CE-01…CE-17 的可执行核心已进入回归脚本或证明套件。CE-07 的「不得绿」由 A8 与判断规则保证：没有 \(unrecoverable\) 覆盖时 \(\widehat\rho\neq 0\)。

## 21. 机器验证

| 方法 | 适合的对象 | 本文状态 |
| --- | --- | --- |
| 有限模型回归 | 格、图、Merkle、判断、挂起、快照、Capability | 已做 |
| TLA+／PlusCal | 并发 Change／Publish、事件丢失、候选反复作废 | 未做 |
| Alloy | 更大图上的成员完整性与 Principal 不变量 | 未做 |
| Tamarin／ProVerif | 握手、重放、TURN、Capability 不可伪造 | 未做 |
| Lean／Coq | 格、图划分、Merkle、Thm 7 的抽象证明 | 未做 |

先运行 [proof/check_proofs.py](proof/check_proofs.py)，再运行 [check_formal_model.py](check_formal_model.py)。

证明套件遵守三条结构纪律，防止「守卫即模型」的空洞检查：

1. 可执行守卫（Exec 前置、时钟、世代交集、发布前置、Ready）在 [proof/semantics.py](proof/semantics.py) 单点定义，STS 与 Z3 共用，手写副本间不可能漂移；
2. 安全不变量在各模型中独立重算，不复用守卫返回值；对抗性迁移总是「尝试」坏行为，由守卫决定放行；
3. [proof/mutants.py](proof/mutants.py) 把守卫逐一换成已知坏定义（去掉过期检查、允许拨回时钟、去掉世代交集、忽略修订号、忽略当前指针、去掉依赖检查、单方 MAC、最弱项换最强项），断言对应检查器必须报警。任一变异体存活，整个套件失败。

有界搜索触及状态上限即显式失败，不存在静默截断。

[check_formal_model.py](check_formal_model.py) 穷举或定向检查：

- 三值聚合的交换／结合／幂等，以及排列无关；
- 增加或削弱节点的单调性；
- 颜色对风险接受和输入授权的独立，以及无效通道／图不可用 \(w\) 绕过；
- `Retain` 映射；
- 最多四节点全部有向图的连通分量，以及「只沿出边」相对完整分量的包含关系；
- 跨 Principal 边不入图，且图外 Actor 出口不得为 \(\mathbf C\)；
- 证据过期、错绑、不受信、真伪冲突时的判断；
- Merkle 成员证明与换叶攻击；
- Capability 字段扩大／错绑／撤销／过期；修订号回滚不复活旧凭证；
- 挂起隔离与期限后拒绝；
- 混合快照与重复 `ActorId` 叶子拒绝发布；
- `EncGraph` 单射、排列无关与标识符字符集约束；
- 数据世代交集；验证者最弱项；本地时钟；Depends／LocalContinue。

本次运行通过脚本打印的断言数。该结果用于防止定义和可执行模型漂移，不能替代 A1–A13 的现实证据。

## 22. 仍未覆盖

以下内容有意保持为假设、产品规则或后续专项，而不是假装已证：

1. 具体 Host 是否实现了 A2（尤其是普通 Native 和 GPU／NPU 路径）；
2. 具体 Actr 握手在符号模型下的保密性与认证性；
3. 发布协议在交错执行下的线性化／活性（TLA+）；
4. 算法迁移和跨语言实现是否遵守已冻结的 `EncGraph`（规范本身已定义）；
5. 可信时间源的硬件与抗回拨实现（模型已改用本地 \(now_a\)，硬件仍是 A8）；
6. 「密钥不可恢复」的密码学与平台证明；
7. 侧信道、流量分析、模型记忆和物理攻击；
8. PrincipalId 的生成、认证与恢复。

PO-01…PO-12 仍是实现要逐项交付的义务。最难的三处不变：Native 闭合、远端加速器组合证明、分布式无根图完整发现。

## 23. 可行性与最终判断

以下组合可以同时成立，不是互相冲突的要求：

- 图无根，但 Tab 可以用 Front Actor 定位；
- 状态本地原子变化，应用结论异步重建；
- Assurance 可复制，但不产生权限；
- Capability 可离线验证，撤销感知受有限期约束；
- 断网后可以继续有限计算，期限后不依赖远端；
- \(q\) 影响颜色，\(w\) 不影响颜色；
- Actor 通道失败直接使 \(color=\bot\)，不参加评分。

### 严谨性

主要概念有明确作用域：Actor 是唯一节点；应用是无根依赖连通组；能力只有 `Transfer`／`Retain`；事实、信任、授权和风险接受分离；四对象闭合绑定；颜色是当前就绪切面上的全量弱项汇聚。判断关系、迁移系统和发布前置已经给出，不再只靠散文约定 \(\vdash\)。

在这些定义内，没有发现需要平均分、循环自证或「不入图就不计分」的内部漏洞。

### 可证明性

绿色可以形式推出，但严格依赖 A2、A3、A4、A5、A6、A8、A9。前提不成立时，系统必须保持黄／红或拒绝建立结论。这是条件安全，不是绝对安全。

### 可计算性

有限快照上主体线性、成员证明对数，不需要全局共识或唯一中央存储。持续变化只影响活性。

综合：

> Actrium 当前方案是一个逻辑自洽、可计算、可对核心代数和对象绑定做有限机械化回归、并能诚实表达未知的条件安全体系。它不能把现实实现自动变安全，但能够规定实现必须证明什么；任何必要证明缺失时，不得给出过强结论。
