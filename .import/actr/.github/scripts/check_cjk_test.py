#!/usr/bin/env python3
# cjk-allow-file: this file is the checker's own test corpus; its fixtures must
# contain literal CJK to exercise the detector, and escapes would defeat them.
"""Negative tests for the CJK checker's pragma handling.

The two cases that must NOT be exempt are the heart of the rule:
  - a pragma with no reason (bare ``cjk-allow:``)
  - a file pragma declared past the scan window

Run: python .github/scripts/check_cjk_test.py
"""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHECKER = ROOT / ".github/scripts/check_cjk.py"

# Written as escapes, not literals, so this test file passes the checker itself.
CJK_PROSE = "# 这是中文注释  # Chinese prose"
CJK_LINE = "let s = \"你好\";"


def _make_repo(tmpdir: Path) -> Path:
    subprocess.run(["git", "init", "-q"], cwd=tmpdir, check=True)
    subprocess.run(
        ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"],
        cwd=tmpdir, check=True,
    )
    return tmpdir


def _run_checker(repo: Path) -> dict:
    result = subprocess.run(
        [sys.executable, str(CHECKER), "--json"],
        cwd=repo, capture_output=True, text=True,
    )
    assert result.returncode in (0, 1), f"checker crashed: {result.stderr}"
    return json.loads(result.stdout)


def test_bare_line_pragma_without_reason_is_flagged() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        (repo / "bare.rs").write_text(
            f"// cjk-allow:\n{CJK_PROSE}\n", encoding="utf-8"
        )
        subprocess.run(["git", "add", "bare.rs"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "bare.rs" in files, "bare cjk-allow: with no reason must be flagged"


def test_line_pragma_with_reason_exempts_next_line() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        (repo / "lined.rs").write_text(
            f"// cjk-allow: parsing wide chars in this token\n{CJK_LINE}\n", encoding="utf-8"
        )
        subprocess.run(["git", "add", "lined.rs"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "lined.rs" not in files, "cjk-allow: <reason> must exempt the next line"


def test_file_pragma_with_reason_exempts_whole_file() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        (repo / "filed.rs").write_text(
            f"// cjk-allow-file: this crate parses CJK headers\n{CJK_PROSE}\n{CJK_LINE}\n",
            encoding="utf-8",
        )
        subprocess.run(["git", "add", "filed.rs"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "filed.rs" not in files, "cjk-allow-file: <reason> must exempt the file"


def test_file_pragma_past_scan_window_is_not_honored() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        # 50 blank-ish lines before the pragma - past the 40-line scan window.
        content = "\n".join(["// padding"] * 50)
        content += "\n// cjk-allow-file: declared too late\n" + CJK_PROSE + "\n"
        (repo / "late.rs").write_text(content, encoding="utf-8")
        subprocess.run(["git", "add", "late.rs"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "late.rs" in files, "file pragma past scan window must not exempt"


def test_zh_md_suffix_is_allowed() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        (repo / "doc.zh.md").write_text(CJK_PROSE + "\n", encoding="utf-8")
        subprocess.run(["git", "add", "doc.zh.md"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "doc.zh.md" not in files, ".zh.md must be allowed"


def test_allowed_dirs_are_skipped() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        (repo / "tests/fixtures").mkdir(parents=True)
        (repo / "tests/fixtures/case.rs").write_text(CJK_PROSE + "\n", encoding="utf-8")
        subprocess.run(["git", "add", "tests/fixtures/case.rs"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "tests/fixtures/case.rs" not in files, "tests/fixtures/ must be allowed"


def test_markdown_language_switcher_link_is_allowed() -> None:
    with tempfile.TemporaryDirectory() as td:
        repo = _make_repo(Path(td))
        # English README whose only CJK is the switcher label pointing at the
        # .zh.md counterpart. This is navigation, not prose.
        content = "# Project\n\n[Chinese](README.zh.md) | English\n"
        (repo / "README.md").write_text(content, encoding="utf-8")
        subprocess.run(["git", "add", "README.md"], cwd=repo, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "x"],
            cwd=repo, check=True,
        )
        data = _run_checker(repo)
        files = {f["file"] for f in data["findings"]}
        assert "README.md" not in files, "language switcher link to .zh.md must be allowed"


def main() -> int:
    tests = [
        test_bare_line_pragma_without_reason_is_flagged,
        test_line_pragma_with_reason_exempts_next_line,
        test_file_pragma_with_reason_exempts_whole_file,
        test_file_pragma_past_scan_window_is_not_honored,
        test_zh_md_suffix_is_allowed,
        test_allowed_dirs_are_skipped,
        test_markdown_language_switcher_link_is_allowed,
    ]
    failures = 0
    for t in tests:
        try:
            t()
            print(f"  PASS  {t.__name__}")
        except AssertionError as e:
            failures += 1
            print(f"  FAIL  {t.__name__}: {e}")
    print(f"\n{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
