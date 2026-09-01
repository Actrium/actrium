# 机器核验

```bash
python3 docs/security/formal/proof/check_proofs.py
python3 docs/security/formal/check_formal_model.py
```

依赖：Python 3.10+，`pip install "z3-solver>=4.12"`。z3 缺失时 check_proofs 直接报错（有意 fail-loud，不静默跳过）。建议把上面两条命令接入 CI：回归脚本不跑等于没有。

| 模块 | 作用 |
| --- | --- |
| `semantics.py` | 共享守卫单一定义源：Exec／时钟／世代／发布／Ready，STS 与 Z3 共用，防手写副本漂移 |
| `kernel.py` | 结果登记核：定理条目只能来自有限判定或登记公理。不是 LCF——公式是标签，语义在判定谓词里 |
| `theory.py` | 有限载体上完全判定 Thm 2–8 及 A1–A13 |
| `z3_theory.py` | Z3 量词证明：格、绿色相对安全、挂起、过期、自签、混合快照、旧 sid 重放；守卫取自 `semantics.py`，部分条目为定义一致性锁定 |
| `sts.py` | 有界迁移系统 BFS：主模型（旧快照保留、由 Ready 指针核对拒绝）＋数据世代＋对抗性本地时钟＋回滚复活；触及状态上限即失败 |
| `channel.py` | 联合 transcript 握手的 Dolev-Yao 穷举：双侧独立视图，旧会话 MAC 重放语料，两侧一致性是被检验结论 |
| `mutants.py` | 变异回归：8 个已知坏定义（去过期检查、允许拨回、去世代交集、忽略修订号、忽略当前指针、去依赖检查、单方 MAC、最弱项换最强项）必须全部被检出 |

本机未安装 Lean／TLA+／Tamarin。无界归纳与计算密码归约见 [../defects.zh.md](../defects.zh.md)。
