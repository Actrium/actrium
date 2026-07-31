#!/usr/bin/env bun
/**
 * Enforce English-only source (issue #90).
 *
 * Source and prose in this repository are English. This scans tracked files for
 * CJK characters and fails on any that is not covered by one of the exemptions
 * below, so a non-English commit is caught at the pull request rather than
 * discovered months later.
 *
 * Exempt without a pragma:
 *   - `*.zh.md` -- the Chinese half of a translated pair
 *   - i18n / locales / translations directories
 *   - test fixture and testdata directories, where CJK is the subject matter
 *   - committed build output (src/guest/build), a transpiled copy of source
 *     that is already checked
 *   - untracked files: only what git tracks is scanned
 *   - every extension not listed in POLICED_EXTENSIONS. `.json`, `.svg` and
 *     captured fixtures carry CJK as data, not as prose a reviewer must read.
 *
 * Exempt with a pragma, which always needs a reason -- the reason is what a
 * reviewer uses to judge whether the exemption is legitimate:
 *   cjk-allow: <reason>        the line it is on, and the line after it
 *   cjk-allow-file: <reason>   the whole file; must appear in the first
 *                              FILE_PRAGMA_WINDOW lines
 *
 * The comment syntax does not matter (`//`, `#`, `<!-- -->`); only the token is
 * matched, so the same pragma works in TypeScript, shell and Markdown.
 *
 * Genuinely-needed CJK in code is better written as a numeric escape -- a
 * fullwidth comma as the six ASCII characters backslash-u-f-f-0-c, not as the
 * character itself. An escape passes this check without a pragma and survives
 * tooling that refuses to write CJK bytes. Use it for single characters and
 * data, never for sentences: escaped prose is unreadable. Note that this file
 * identifies CJK by code point rather than by a literal character class, so it
 * does not flag itself.
 */

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

/**
 * Inclusive code-point ranges treated as CJK. Expressed numerically so this
 * file contains no CJK bytes of its own -- a literal character class here would
 * make the checker its own first violation.
 */
const CJK_RANGES: ReadonlyArray<readonly [number, number]> = [
  [0x3000, 0x303f], // CJK symbols and punctuation
  [0x3040, 0x30ff], // hiragana + katakana
  [0x31f0, 0x31ff], // katakana phonetic extensions
  [0x3400, 0x4dbf], // CJK extension A
  [0x4e00, 0x9fff], // CJK unified ideographs
  [0xac00, 0xd7af], // Hangul syllables
  [0xf900, 0xfaff], // CJK compatibility ideographs
  [0xff01, 0xff60], // fullwidth forms
  [0xffe0, 0xffe6], // fullwidth currency and bar signs
];

function hasCjk(text: string): boolean {
  for (const character of text) {
    const codePoint = character.codePointAt(0);
    if (codePoint === undefined) continue;
    for (const [low, high] of CJK_RANGES) {
      if (codePoint >= low && codePoint <= high) return true;
    }
  }
  return false;
}

/** Extensions worth policing: source, scripts, config and English prose. */
const POLICED_EXTENSIONS = [
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.rs',
  '.swift',
  '.kt',
  '.java',
  '.m',
  '.mm',
  '.c',
  '.cc',
  '.cpp',
  '.h',
  '.hpp',
  '.sh',
  '.bash',
  '.py',
  '.rb',
  '.toml',
  '.yml',
  '.yaml',
  '.md',
];

/**
 * Path fragments where CJK is expected. Build output is matched by its specific
 * directory rather than a bare `build/` or `lib/`, since hand-written
 * directories by those names are common.
 */
const EXEMPT_PATH_FRAGMENTS = [
  'i18n/',
  'locales/',
  'translations/',
  '__tests__/fixtures/',
  'testdata/',
  'src/guest/build/',
];

/** How far into a file a `cjk-allow-file:` declaration is honoured. */
const FILE_PRAGMA_WINDOW = 20;

const LINE_PRAGMA = /cjk-allow:[ \t]*(\S.*)?$/;
const FILE_PRAGMA = /cjk-allow-file:[ \t]*(\S.*)?$/;

/** A markdown link whose target is the `.zh.md` counterpart of this document. */
const ZH_SWITCHER_LINK = /\[[^\]]*\]\([^)]*\.zh\.md\)/g;

export interface Violation {
  file: string;
  line: number;
  kind: 'cjk' | 'empty-reason';
  text: string;
}

export function isPoliced(path: string): boolean {
  if (path.endsWith('.zh.md')) return false;
  if (EXEMPT_PATH_FRAGMENTS.some((fragment) => path.includes(fragment))) return false;
  return POLICED_EXTENSIONS.some((extension) => path.endsWith(extension));
}

/**
 * Scan one file's contents. Separate from the CLI so the pragma rules can be
 * tested directly, including the cases that must FAIL.
 */
export function scanText(file: string, content: string): Violation[] {
  const violations: Violation[] = [];
  const lines = content.split('\n');
  const isMarkdown = file.endsWith('.md');

  // A file pragma exempts everything, but only if it is declared up top where a
  // reader will see it. Past the window it does not count -- and if it carries
  // no reason it is an error in its own right, wherever it sits.
  let fileExempt = false;
  for (let i = 0; i < lines.length; i++) {
    const match = FILE_PRAGMA.exec(lines[i]);
    if (!match) continue;
    if (!match[1]) {
      violations.push({ file, line: i + 1, kind: 'empty-reason', text: lines[i].trim() });
      continue;
    }
    if (i < FILE_PRAGMA_WINDOW) fileExempt = true;
  }

  let exemptUntil = -1;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const linePragma = LINE_PRAGMA.exec(raw);
    if (linePragma) {
      if (linePragma[1]) {
        exemptUntil = i + 1; // this line and the next
      } else {
        violations.push({ file, line: i + 1, kind: 'empty-reason', text: raw.trim() });
      }
    }

    if (fileExempt || i <= exemptUntil) continue;

    // Discount only the pointer at the Chinese counterpart, so real Chinese
    // prose in an English document still fails.
    const candidate = isMarkdown ? raw.replace(ZH_SWITCHER_LINK, '') : raw;
    if (hasCjk(candidate)) {
      violations.push({ file, line: i + 1, kind: 'cjk', text: raw.trim() });
    }
  }

  return violations;
}

function trackedFiles(): string[] {
  const out = execFileSync('git', ['ls-files', '-z'], {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  return out.split('\0').filter(Boolean);
}

function main(): void {
  const violations: Violation[] = [];
  for (const file of trackedFiles()) {
    if (!isPoliced(file)) continue;
    let content: string;
    try {
      content = readFileSync(file, 'utf8');
    } catch {
      continue; // deleted from the worktree but still in the index
    }
    violations.push(...scanText(file, content));
  }

  if (violations.length === 0) {
    console.log('check:cjk -- source is English-only.');
    return;
  }

  const cjk = violations.filter((v) => v.kind === 'cjk');
  const emptyReason = violations.filter((v) => v.kind === 'empty-reason');

  for (const v of violations) {
    const why =
      v.kind === 'empty-reason'
        ? 'exemption pragma without a reason'
        : 'CJK in a file that must be English';
    console.log(`${v.file}:${v.line}: ${why}`);
    console.log(`  ${v.text}`);
  }

  console.log('');
  console.log(
    `check:cjk -- ${cjk.length} CJK violation(s), ${emptyReason.length} pragma(s) missing a reason.`
  );
  console.log('Translate the text, move it to a *.zh.md file, or exempt it with');
  console.log('  cjk-allow: <why this one is legitimate>');
  console.log('  cjk-allow-file: <why this whole file is legitimate>   (near the top)');

  // Not process.exit(): that truncates buffered stdout on a pipe, which silently
  // cuts the report short.
  process.exitCode = 1;
}

if (import.meta.main) main();
