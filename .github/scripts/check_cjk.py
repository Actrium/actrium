#!/usr/bin/env python3
"""Enforce English-only source. CJK belongs in a small set of places; anywhere
else it is a defect, because a reader who does not read Chinese cannot review
the code, and mixed-language identifiers and messages leak into product output.

Where CJK IS allowed:
  - *.zh.md                        translated docs, named so the language is explicit
  - i18n/ locales/ translations/   localisation payloads
  - tests/fixtures/ testdata/      test inputs
  - anything git does not track    build output, vendored downloads

Where it is allowed by exception, with a written reason:
  # cjk-allow: <reason>           the same line and the line after it
  # cjk-allow-file: <reason>      the whole file, must sit in the first 40 lines

A pragma without a reason does not count. The reason is the point: it is what
a reviewer reads to decide whether the exception is legitimate, so "cjk-allow"
on its own is rejected the same as no pragma at all.

  python .github/scripts/check_cjk.py            gate; exits 1 on any violation
  python .github/scripts/check_cjk.py --report   inventory, always exits 0
  python .github/scripts/check_cjk.py --json     machine-readable findings
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

# Ideographic and Hangul blocks, plus the fullwidth forms that come with them.
# Deliberately excludes U+2018/2019/201C/201D - curly quotes are common in
# English prose and are not a language marker.
# Written as Unicode escape sequences (\uXXXX), not literals, so this file
# passes its own check without needing a pragma exemption.
CJK_PATTERN = re.compile(
    "[" +
    r"\u3000-\u303f"  # CJK symbols and punctuation
    r"\u3040-\u309f"  # hiragana
    r"\u30a0-\u30ff"  # katakana
    r"\u3400-\u4dbf"  # CJK unified ideographs extension A
    r"\u4e00-\u9fff"  # CJK unified ideographs
    r"\uf900-\ufaff"  # CJK compatibility ideographs
    r"\uff00-\uffef"  # halfwidth and fullwidth forms
    r"\uac00-\ud7af"  # hangul syllables
    "]"
)

ALLOWED_DIRS = (
    "i18n/",
    "locales/",
    "translations/",
    "tests/fixtures/",
    "testdata/",
    "tests/cases/_fixtures/",
)

# Committed build output. Its comments are a transpiled copy of the source that
# was already checked, so flagging it would ask for the same text to be fixed
# twice - and the second copy cannot be hand-fixed safely, because editing it
# shifts the line numbers its sourcemap is keyed to.
BUILD_OUTPUT_DIRS = (
    "lib/commonjs/",
    "lib/module/",
    "lib/typescript/",
    "dist/",
    "build/",
)

# Extensions that are reviewed as source. Everything else (.svg, .png, .json
# data) is left alone: diagram sources and captured fixtures carry CJK as
# subject matter, not as prose a reviewer has to read.
SOURCE_EXTENSIONS = {
    ".rs", ".go", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
    ".py", ".java", ".kt", ".kts", ".swift", ".c", ".h", ".cpp", ".hpp", ".cc",
    ".m", ".mm", ".sh", ".bash", ".zsh", ".rb", ".php", ".cs", ".vue", ".svelte",
    ".css", ".scss", ".less", ".toml", ".yaml", ".yml", ".html", ".podspec",
    ".gradle", ".cmake", ".mk", ".proto", ".wit", ".sql",
}

LINE_PRAGMA = re.compile(r"cjk-allow:\s*\S")
FILE_PRAGMA = re.compile(r"cjk-allow-file:\s*\S")
FILE_PRAGMA_SCAN_LINES = 40

LANGUAGE_SWITCHER_RE = re.compile(r"\[[^\]]*\]\(([^)]*\.zh\.md)\)")


def _git_tracked_files() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(f"git ls-files failed: {result.stderr.strip()}\n")
        sys.exit(2)
    return [f for f in result.stdout.split("\0") if f]


def _read_text(path: str) -> str | None:
    try:
        raw = Path(path).read_bytes()
    except OSError:
        return None
    if b"\x00" in raw:
        return None  # binary
    return raw.decode("utf-8", errors="replace")


def scan() -> tuple[list[dict], dict]:
    findings: list[dict] = []
    skipped = {
        "allowedLocation": 0,
        "buildOutput": 0,
        "notSource": 0,
        "pragmaFile": 0,
        "binary": 0,
    }

    for posix in _git_tracked_files():
        if any(posix == d.rstrip("/") or d in posix for d in ALLOWED_DIRS):
            skipped["allowedLocation"] += 1
            continue
        if posix.endswith(".zh.md"):
            skipped["allowedLocation"] += 1
            continue
        if any(d in posix for d in BUILD_OUTPUT_DIRS):
            skipped["buildOutput"] += 1
            continue

        ext = Path(posix).suffix
        is_markdown = ext == ".md"
        if not is_markdown and ext not in SOURCE_EXTENSIONS:
            skipped["notSource"] += 1
            continue

        text = _read_text(posix)
        if text is None:
            skipped["binary"] += 1
            continue
        if not CJK_PATTERN.search(text):
            continue

        lines = text.splitlines()

        if any(FILE_PRAGMA.search(line) for line in lines[:FILE_PRAGMA_SCAN_LINES]):
            skipped["pragmaFile"] += 1
            continue

        if is_markdown:
            without_switcher = LANGUAGE_SWITCHER_RE.sub("", text)
            markdown_lines = without_switcher.splitlines()
            offending_lines = [
                line
                for index, line in enumerate(markdown_lines)
                if CJK_PATTERN.search(line)
                and not LINE_PRAGMA.search(line)
                and not (index > 0 and LINE_PRAGMA.search(markdown_lines[index - 1]))
            ]
            if not offending_lines:
                skipped["allowedLocation"] += 1
                continue
            count = sum(len(CJK_PATTERN.findall(line)) for line in offending_lines)
            renamed = re.sub(r"\.md$", ".zh.md", posix)
            findings.append({
                "file": posix,
                "kind": "markdown-should-be-zh-md",
                "chars": count,
                "hint": f"rename to {renamed}, or translate it to English in place",
            })
            continue

        for index, line in enumerate(lines):
            if not CJK_PATTERN.search(line):
                continue
            if LINE_PRAGMA.search(line):
                continue
            if index > 0 and LINE_PRAGMA.search(lines[index - 1]):
                continue
            findings.append({
                "file": posix,
                "line": index + 1,
                "kind": "cjk-in-source",
                "text": line.strip()[:160],
            })

    return findings, skipped


def _print_human(findings: list[dict], skipped: dict) -> None:
    by_file: dict[str, list[dict]] = {}
    for f in findings:
        by_file.setdefault(f["file"], []).append(f)

    markdown_files = [(p, fs) for p, fs in by_file.items() if fs[0]["kind"] == "markdown-should-be-zh-md"]
    source_files = [(p, fs) for p, fs in by_file.items() if fs[0]["kind"] == "cjk-in-source"]

    if source_files:
        total_lines = sum(len(fs) for _, fs in source_files)
        print(f"\nCJK in source ({len(source_files)} files, {total_lines} lines):\n")
        for file, entries in sorted(source_files, key=lambda kv: -len(kv[1])):
            print(f"  {file}  ({len(entries)} line{'s' if len(entries) != 1 else ''})")
            for entry in entries[:3]:
                print(f"      {entry['line']}: {entry['text']}")
            if len(entries) > 3:
                print(f"      ... {len(entries) - 3} more")

    if markdown_files:
        print(f"\nChinese Markdown not named .zh.md ({len(markdown_files)} files):\n")
        for file, entries in sorted(markdown_files, key=lambda kv: -kv[1][0]["chars"]):
            print(f"  {file}  ({entries[0]['chars']} CJK chars)")

    if not findings:
        print("check-cjk: clean - no CJK outside the allowed locations.")
    else:
        print(
            f"\ncheck-cjk: {len(findings)} finding(s) across {len(by_file)} file(s).\n"
            "\nTo resolve, pick the one that is actually true:\n"
            "  - prose meant for readers      translate it to English\n"
            "  - a Chinese document           rename to *.zh.md\n"
            '  - CJK as data, not prose       add "cjk-allow: <reason>" above the line,\n'
            '                                 or "cjk-allow-file: <reason>" near the top'
        )

    print(
        f"\nskipped: {skipped['allowedLocation']} allowed location, "
        f"{skipped['buildOutput']} build output, {skipped['notSource']} not source, "
        f"{skipped['pragmaFile']} file pragma, {skipped['binary']} binary"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Enforce English-only source.")
    parser.add_argument("--report", action="store_true", help="inventory; always exit 0")
    parser.add_argument("--json", action="store_true", help="machine-readable findings")
    args = parser.parse_args()

    findings, skipped = scan()
    exit_code = 0 if (args.report or not findings) else 1

    if args.json:
        print(json.dumps({"findings": findings, "skipped": skipped}, indent=2))
    else:
        _print_human(findings, skipped)

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
