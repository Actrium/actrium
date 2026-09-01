# Machine verification

```bash
python3 docs/security/formal/proof/check_proofs.py
python3 docs/security/formal/check_formal_model.py
```

Dependency:Python 3.10+, `pip install "z3-solver>=4.12"`.z3 When missing check_proofs Directly misbehavioured (intentional) fail-loud, skip silently. It is recommended that the two commands above be connected. CI: Returning to the script is equal to not running.

| Modules | Role |
| --- | --- |
| `semantics.py` | Shared guard single defined source:Exec/Clock/ Generation/Issuing/Ready, STS with Z3 Shared, handwritten copy drifts |
| `kernel.py` | Result registration: Theorem entry can only be from limited determination or registration justice. LCF——The formula is a label. Semantics are in the word. |
| `theory.py` | Full determination on limited carrier. Thm 2–8 and A1–A13 |
| `z3_theory.py` | Z3 Quantified: grid, green relative safety, hang-up, expired, self-signed, mixed snapshot, old sid Replay;guards taken from `semantics.py`, some entries are defined to be locked in |
| `sts.py` | SRT system BFS: Main model (old snapshots retained, by Ready Pointer check denied) + Data Generation + Opposive Local Clock + Rollback; Reaching the state ceiling is a failure |
| `channel.py` | United Nations transcript Shake hands. Dolev-Yao Overlapping: Two-sided independent view, old session MAC Re-speak. Consistency on both sides is the result of the test. |
| `mutants.py` | Mutation regression:8 Known bad definitions (defunct check, permission to dial back, intergenerational exchange, neglect of revision numbers, neglect of current pointer, reliance on inspection, unilateral MACThe weakest for the strongest) must be checked out. |

Not installed on-site Lean/TLA+/Tamarin. Undefined summarizing and calculating passwords are agreed upon [../defects.zh.md](../defects.zh.md).
