#!/usr/bin/env python3
"""Enforce the repository language and filename policy.

English is the default language for every tracked text file. Translations must
live in a path that identifies their locale, for example:

  README.zh.md
  locales/zh-Hans/messages.json
  android/res/values-zh-rCN/strings.xml
  ios/zh-Hans.lproj/Localizable.strings

Tracked paths must be ASCII so that they are portable and typeable on every
supported development host. The content check covers all tracked UTF-8 text,
not only source and Markdown extensions. It detects scripts that unambiguously
mark non-English prose; Latin-script language detection is intentionally left
to review because names, citations, and English loanwords make it unreliable.

Literal non-English text that is genuinely required by product or protocol
semantics, and cannot live in a locale resource, needs a reviewable reason:

  # language-allow: <reason>       the same line and the line after it
  # language-allow-file: <reason>  the whole file; within the first 40 lines

The older cjk-allow spellings remain accepted for compatibility.

  python .github/scripts/check_cjk.py            gate; exits 1 on violations
  python .github/scripts/check_cjk.py --report   inventory; always exits 0
  python .github/scripts/check_cjk.py --json     machine-readable findings
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import unicodedata
from pathlib import Path, PurePosixPath


# Scripts that unambiguously indicate non-English text. Unicode escapes keep
# this checker compliant with its own policy. Curly quotes, mathematical
# symbols, emoji, and Latin letters with diacritics are deliberately excluded.
NON_ENGLISH_SCRIPT_PATTERN = re.compile(
    "["
    + r"\u0370-\u03ff"  # Greek and Coptic
    + r"\u0400-\u052f"  # Cyrillic and Cyrillic supplement
    + r"\u0530-\u058f"  # Armenian
    + r"\u0590-\u05ff"  # Hebrew
    + r"\u0600-\u06ff"  # Arabic
    + r"\u0700-\u074f"  # Syriac
    + r"\u0750-\u077f"  # Arabic supplement
    + r"\u0780-\u07bf"  # Thaana
    + r"\u08a0-\u08ff"  # Arabic extended
    + r"\u0900-\u0dff"  # Indic scripts
    + r"\u0e00-\u0e7f"  # Thai and Lao
    + r"\u0f00-\u0fff"  # Tibetan
    + r"\u1000-\u109f"  # Myanmar
    + r"\u10a0-\u10ff"  # Georgian
    + r"\u1200-\u137f"  # Ethiopic
    + r"\u1780-\u17ff"  # Khmer
    + r"\u3000-\u303f"  # CJK symbols and punctuation
    + r"\u3040-\u30ff"  # Hiragana and Katakana
    + r"\u3100-\u312f"  # Bopomofo
    + r"\u3400-\u4dbf"  # CJK unified ideographs extension A
    + r"\u4e00-\u9fff"  # CJK unified ideographs
    + r"\uac00-\ud7af"  # Hangul syllables
    + r"\uf900-\ufaff"  # CJK compatibility ideographs
    + r"\uff00-\uffef"  # Halfwidth and fullwidth forms
    + "]"
)


def _non_english_characters(text: str) -> list[str]:
    """Return characters that cannot occur in unmarked English prose.

    In addition to the explicit script ranges above, reject non-ASCII Unicode
    letters and combining marks. This catches Latin-script translations such as
    French or Vietnamese while still allowing English typography, emoji, and
    symbols.
    """
    emoji_format_characters = {"\u2139", "\ufe0e", "\ufe0f", "\u20e3"}
    mathematical_letter_symbols = {"\u00b5", "\u0192"}
    allowed_non_language_characters = emoji_format_characters | mathematical_letter_symbols
    return [
        char
        for char in text
        if char not in allowed_non_language_characters
        and (
            NON_ENGLISH_SCRIPT_PATTERN.match(char)
            or (ord(char) > 0x7F and unicodedata.category(char)[0] in {"L", "M"})
        )
    ]


def _contains_non_english_text(text: str) -> bool:
    return bool(_non_english_characters(text))

# Common BCP 47 shapes used in filenames and resource directory names. Locale
# recognition is intentionally contextual so an ordinary two-letter directory
# such as `go/` is not accidentally treated as a translation directory.
LOCALE_TAG = r"[a-z]{2,3}(?:-[A-Za-z0-9]{2,8})*"
LOCALIZED_FILENAME = re.compile(
    rf"^.+\.(?P<locale>{LOCALE_TAG})\.[^.]+$", re.IGNORECASE
)
LOCALE_CONTAINER_NAMES = {"i18n", "l10n", "locales", "translations"}
ANDROID_LOCALE_DIR = re.compile(rf"^values-(?:b\+)?{LOCALE_TAG}$", re.IGNORECASE)
APPLE_LOCALE_DIR = re.compile(rf"^{LOCALE_TAG}\.lproj$", re.IGNORECASE)

LINE_PRAGMA = re.compile(r"(?:language|cjk)-allow:\s*\S")
FILE_PRAGMA = re.compile(r"(?:language|cjk)-allow-file:\s*\S")
FILE_PRAGMA_SCAN_LINES = 40

LANGUAGE_SWITCHER_RE = re.compile(
    rf"\[[^\]]*\]\(([^)]*\.{LOCALE_TAG}\.md(?:#[^)]*)?)\)", re.IGNORECASE
)


def _git_tracked_files() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True
    )
    if result.returncode != 0:
        sys.stderr.write(f"git ls-files failed: {result.stderr.strip()}\n")
        sys.exit(2)
    return [path for path in result.stdout.split("\0") if path]


def _read_text(path: str) -> str | None:
    try:
        raw = Path(path).read_bytes()
    except OSError:
        return None
    if b"\x00" in raw:
        return None
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return None


def _is_localized_path(path: str) -> bool:
    parts = PurePosixPath(path).parts
    filename = parts[-1]

    if LOCALIZED_FILENAME.search(filename):
        return True
    if len(parts) > 1 and parts[-2].lower() in LOCALE_CONTAINER_NAMES:
        locale_stem = filename.rsplit(".", 1)[0]
        if re.fullmatch(LOCALE_TAG, locale_stem, re.IGNORECASE):
            return True

    for index, part in enumerate(parts[:-1]):
        if APPLE_LOCALE_DIR.fullmatch(part) or ANDROID_LOCALE_DIR.fullmatch(part):
            return True
        if index > 0 and parts[index - 1].lower() in LOCALE_CONTAINER_NAMES:
            if re.fullmatch(LOCALE_TAG, part, re.IGNORECASE):
                return True
    return False


def _line_is_allowed(lines: list[str], index: int) -> bool:
    return bool(
        LINE_PRAGMA.search(lines[index])
        or (index > 0 and LINE_PRAGMA.search(lines[index - 1]))
    )


def scan() -> tuple[list[dict], dict]:
    findings: list[dict] = []
    skipped = {"localized": 0, "pragmaFile": 0, "binary": 0}

    for posix in _git_tracked_files():
        if not posix.isascii():
            findings.append({
                "file": posix,
                "kind": "non-ascii-path",
                "hint": "rename the path using portable ASCII characters",
            })

        if _is_localized_path(posix):
            skipped["localized"] += 1
            continue

        text = _read_text(posix)
        if text is None:
            skipped["binary"] += 1
            continue
        if not _contains_non_english_text(text):
            continue

        lines = text.splitlines()
        if any(FILE_PRAGMA.search(line) for line in lines[:FILE_PRAGMA_SCAN_LINES]):
            skipped["pragmaFile"] += 1
            continue

        is_markdown = posix.lower().endswith(".md")
        if is_markdown:
            without_switcher = LANGUAGE_SWITCHER_RE.sub("", text)
            markdown_lines = without_switcher.splitlines()
            offending = [
                line
                for index, line in enumerate(markdown_lines)
                if _contains_non_english_text(line)
                and not _line_is_allowed(markdown_lines, index)
            ]
            if offending:
                count = sum(
                    len(_non_english_characters(line)) for line in offending
                )
                findings.append({
                    "file": posix,
                    "kind": "default-markdown-is-not-english",
                    "chars": count,
                    "hint": "translate the default document to English and put each translation in a locale-marked file",
                })
            continue

        for index, line in enumerate(lines):
            if not _contains_non_english_text(line):
                continue
            if _line_is_allowed(lines, index):
                continue
            findings.append({
                "file": posix,
                "line": index + 1,
                "kind": "non-english-script-in-default-file",
                "text": line.strip()[:160],
            })

    return findings, skipped


def _print_human(findings: list[dict], skipped: dict) -> None:
    by_file: dict[str, list[dict]] = {}
    for finding in findings:
        by_file.setdefault(finding["file"], []).append(finding)

    path_files = [
        (path, entries)
        for path, entries in by_file.items()
        if any(entry["kind"] == "non-ascii-path" for entry in entries)
    ]
    markdown_files = [
        (path, entries)
        for path, entries in by_file.items()
        if any(entry["kind"] == "default-markdown-is-not-english" for entry in entries)
    ]
    content_files = [
        (path, entries)
        for path, entries in by_file.items()
        if any(entry["kind"] == "non-english-script-in-default-file" for entry in entries)
    ]

    if path_files:
        print(f"\nNon-ASCII tracked paths ({len(path_files)}):\n")
        for path, _ in sorted(path_files):
            print(f"  {path}")

    if content_files:
        line_count = sum(
            sum(entry["kind"] == "non-english-script-in-default-file" for entry in entries)
            for _, entries in content_files
        )
        print(f"\nNon-English script in default files ({len(content_files)} files, {line_count} lines):\n")
        for path, entries in sorted(content_files, key=lambda item: -len(item[1])):
            relevant = [
                entry for entry in entries
                if entry["kind"] == "non-english-script-in-default-file"
            ]
            print(f"  {path}  ({len(relevant)} line{'s' if len(relevant) != 1 else ''})")
            for entry in relevant[:3]:
                print(f"      {entry['line']}: {entry['text']}")
            if len(relevant) > 3:
                print(f"      ... {len(relevant) - 3} more")

    if markdown_files:
        print(f"\nDefault Markdown containing non-English script ({len(markdown_files)}):\n")
        for path, entries in sorted(markdown_files):
            relevant = [
                entry for entry in entries
                if entry["kind"] == "default-markdown-is-not-english"
            ]
            print(f"  {path}  ({relevant[0]['chars']} characters)")

    if not findings:
        print("language-policy: clean")
    else:
        print(
            f"\nlanguage-policy: {len(findings)} finding(s) across {len(by_file)} file(s).\n\n"
            "Resolve each finding by translating the default file to English, "
            "moving translated content to a locale-marked path, renaming a path "
            "to ASCII. A narrowly reasoned language-allow pragma is reserved "
            "for content genuinely required by product or protocol semantics."
        )

    print(
        f"\nskipped: {skipped['localized']} localized file, "
        f"{skipped['pragmaFile']} file pragma, {skipped['binary']} binary/non-UTF-8"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Enforce repository language policy.")
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
