# 机器核验

```bash
python3 docs/security/formal/proof/check_proofs.py
python3 docs/security/formal/check_formal_model.py
```

| 模块 | 作用 |
| --- | --- |
| `kernel.py` | LCF 核：定理只能由规则或登记公理产生 |
| `theory.py` | 有限载体上完全判定 Thm 2–8 及 A1–A12 |
| `z3_theory.py` | Z3 量词证明：格、绿色相对安全、挂起、过期、自签、混合快照、旧 sid 重放 |
| `sts.py` | 有界迁移系统 BFS；另含数据世代与本地时钟专用穷举 |
| `channel.py` | 联合 transcript 握手的 Dolev-Yao 穷举 |

本机未安装 Lean／TLA+／Tamarin。无界归纳与计算密码归约见 [../defects.zh.md](../defects.zh.md)。
