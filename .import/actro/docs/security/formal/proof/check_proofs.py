#!/usr/bin/env python3
"""运行规范理论、有界 STS 与握手穷举。"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from channel import check_channel
from sts import check_sts
from theory import check_theory
from z3_theory import check_z3


def main() -> int:
    report = {
        "theory": check_theory(),
        "z3": check_z3(),
        "sts": check_sts(),
        "channel": check_channel(),
    }
    failed = [
        name
        for name, body in report.items()
        if body.get("status") != "PASS"
    ]
    report["status"] = "PASS" if not failed else "FAIL"
    report["failed"] = failed
    print(json.dumps(report, ensure_ascii=False, indent=2, default=str))
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
