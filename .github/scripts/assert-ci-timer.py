#!/usr/bin/env python3
"""Verify the RFC-0400 timer guard is included in the selected tests."""
import json
import sys
from pathlib import Path


inventory = json.loads(Path(sys.argv[1]).read_text())
name = "timer::tests::inv24_production_timer_calls_and_inventory_do_not_drift"
selected = []
for suite in inventory["rust-suites"].values():
    case = suite.get("testcases", {}).get(name)
    if (suite["package-name"] == "actr-hyper" and suite["kind"] == "lib"
            and case and not case["ignored"] and case["filter-match"]["status"] == "matches"):
        selected.append(suite["binary-id"])
if len(selected) != 1:
    raise SystemExit(f"RFC-0400 timer test must be selected exactly once: {selected}")
print(f"RFC-0400 timer selected in {selected[0]}")
