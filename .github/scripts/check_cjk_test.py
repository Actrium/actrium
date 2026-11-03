#!/usr/bin/env python3
"""Tests for the repository language-policy checker."""
from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHECKER = ROOT / ".github/scripts/check_cjk.py"
CJK_PROSE = "# \u8fd9\u662f\u4e2d\u6587\u6ce8\u91ca"
CYRILLIC_PROSE = "# \u042d\u0442\u043e \u0440\u0443\u0441\u0441\u043a\u0438\u0439 \u0442\u0435\u043a\u0441\u0442"
LATIN_TRANSLATION = "# D\u00e9j\u00e0 vu"


def _make_repo(tmpdir: Path) -> Path:
    subprocess.run(["git", "init", "-q"], cwd=tmpdir, check=True)
    subprocess.run(
        ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"],
        cwd=tmpdir,
        check=True,
    )
    return tmpdir


def _check(files: dict[str, str]) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        repo = _make_repo(Path(tmp))
        for relative, content in files.items():
            path = repo / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        subprocess.run(["git", "add", "--all"], cwd=repo, check=True)
        result = subprocess.run(
            [sys.executable, str(CHECKER), "--json"],
            cwd=repo,
            capture_output=True,
            text=True,
        )
        assert result.returncode in (0, 1), f"checker crashed: {result.stderr}"
        return json.loads(result.stdout)


def _files(data: dict) -> set[str]:
    return {finding["file"] for finding in data["findings"]}


def test_default_source_rejects_cjk() -> None:
    assert "src/main.rs" in _files(_check({"src/main.rs": CJK_PROSE}))


def test_default_markdown_rejects_cjk() -> None:
    assert "README.md" in _files(_check({"README.md": CJK_PROSE}))


def test_other_non_english_script_is_rejected() -> None:
    assert "README.md" in _files(_check({"README.md": CYRILLIC_PROSE}))


def test_unmarked_non_ascii_latin_text_is_rejected() -> None:
    assert "README.md" in _files(_check({"README.md": LATIN_TRANSLATION}))


def test_emoji_and_unit_symbols_are_not_languages() -> None:
    content = "# Status\n\nInformation \u2139\ufe0f, key 1\ufe0f\u20e3, 50 \u00b5s, G.711 \u03bc-law\n"
    assert not _files(_check({"README.md": content}))
    assert _files(_check({"README.md": "\u03bc\u03bf\u03bd\u03ac\u03b4\u03b1"}))


def test_zh_markdown_is_localized() -> None:
    assert not _files(_check({"README.zh.md": CJK_PROSE}))


def test_standard_locale_directories_are_localized() -> None:
    files = {
        "locales/zh-Hans/messages.json": CJK_PROSE,
        "locales/zh-Hans.json": CJK_PROSE,
        "locales/zh.ts": CJK_PROSE,
        "android/res/values-zh-rCN/strings.xml": CJK_PROSE,
        "ios/zh-Hans.lproj/Localizable.strings": CJK_PROSE,
    }
    assert not _files(_check(files))


def test_short_source_stem_is_not_a_locale_suffix() -> None:
    assert _files(_check({"src/ffi.rs": CJK_PROSE})) == {"src/ffi.rs"}
    assert _files(_check({"src/api.ts": CJK_PROSE})) == {"src/api.ts"}
    assert _files(_check({"src/app.tsx": CJK_PROSE})) == {"src/app.tsx"}
    assert not _files(_check({"src/messages.fr.ts": CJK_PROSE}))


def test_unmarked_i18n_and_fixture_files_are_not_blanket_allowed() -> None:
    data = _check({
        "i18n/messages.json": CJK_PROSE,
        "tests/fixtures/case.txt": CJK_PROSE,
    })
    assert _files(data) == {"i18n/messages.json", "tests/fixtures/case.txt"}


def test_non_ascii_path_is_rejected_even_when_localized() -> None:
    data = _check({"locales/zh-Hans/\u8bf4\u660e.md": CJK_PROSE})
    findings = data["findings"]
    assert any(finding["kind"] == "non-ascii-path" for finding in findings)


def test_line_pragma_requires_reason() -> None:
    bare = _check({"bare.rs": f"// language-allow:\n{CJK_PROSE}\n"})
    allowed = _check({
        "allowed.rs": f"// language-allow: literal parser fixture\n{CJK_PROSE}\n"
    })
    assert "bare.rs" in _files(bare)
    assert "allowed.rs" not in _files(allowed)


def test_file_pragma_is_early_and_requires_reason() -> None:
    allowed = _check({
        "allowed.rs": f"// language-allow-file: protocol fixture corpus\n{CJK_PROSE}\n"
    })
    late_content = "\n".join(["// padding"] * 50)
    late_content += f"\n// language-allow-file: too late\n{CJK_PROSE}\n"
    late = _check({"late.rs": late_content})
    assert "allowed.rs" not in _files(allowed)
    assert "late.rs" in _files(late)


def test_language_switcher_label_is_allowed() -> None:
    content = "# Project\n\n[\u4e2d\u6587](README.zh.md) | English\n"
    assert "README.md" not in _files(_check({"README.md": content}))


def main() -> int:
    tests = [
        test_default_source_rejects_cjk,
        test_default_markdown_rejects_cjk,
        test_other_non_english_script_is_rejected,
        test_unmarked_non_ascii_latin_text_is_rejected,
        test_emoji_and_unit_symbols_are_not_languages,
        test_zh_markdown_is_localized,
        test_standard_locale_directories_are_localized,
        test_short_source_stem_is_not_a_locale_suffix,
        test_unmarked_i18n_and_fixture_files_are_not_blanket_allowed,
        test_non_ascii_path_is_rejected_even_when_localized,
        test_line_pragma_requires_reason,
        test_file_pragma_is_early_and_requires_reason,
        test_language_switcher_label_is_allowed,
    ]
    failures = 0
    for test in tests:
        try:
            test()
            print(f"  PASS  {test.__name__}")
        except AssertionError as error:
            failures += 1
            print(f"  FAIL  {test.__name__}: {error}")
    print(f"\n{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
