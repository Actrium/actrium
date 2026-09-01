import { describe, expect, it } from 'bun:test';
import { isPoliced, scanText } from '../check-cjk';

/**
 * CJK sample text built from code points rather than written literally: a
 * literal here would make this file the checker's own first violation. Same
 * reasoning as the character ranges in check-cjk.ts.
 */
const HAN = String.fromCodePoint(0x4e2d, 0x6587); // two Han ideographs
const FULLWIDTH_PAREN = String.fromCodePoint(0xff08, 0xff09); // fullwidth ( )

describe('check:cjk — what counts as a policed file', () => {
  it('polices source and English prose', () => {
    expect(isPoliced('src/shared/types.ts')).toBe(true);
    expect(isPoliced('crates/keel-native-guest/src/lib.rs')).toBe(true);
    expect(isPoliced('README.md')).toBe(true);
    expect(isPoliced('scripts/build.sh')).toBe(true);
  });

  it('leaves the Chinese half of a translated pair alone', () => {
    expect(isPoliced('README.zh.md')).toBe(false);
    expect(isPoliced('docs/reference/guest-lines.zh.md')).toBe(false);
  });

  it('does not police extensions where CJK is data rather than prose', () => {
    expect(isPoliced('contracts/sandbox/canvas-wire.golden.json')).toBe(false);
    expect(isPoliced('assets/logo.svg')).toBe(false);
  });

  it('exempts fixtures, testdata, translation dirs and committed build output', () => {
    expect(isPoliced('src/host/native-guest/__tests__/fixtures/ui-guest.wasm')).toBe(false);
    expect(isPoliced('src/host/__tests__/fixtures/sample.ts')).toBe(false);
    expect(isPoliced('app/testdata/case.ts')).toBe(false);
    expect(isPoliced('src/i18n/zh.ts')).toBe(false);
    expect(isPoliced('src/locales/zh.ts')).toBe(false);
    expect(isPoliced('src/guest/build/bundle.ts')).toBe(false);
  });
});

describe('check:cjk — detection', () => {
  it('flags Han ideographs', () => {
    const v = scanText('a.ts', `// ${HAN}\n`);
    expect(v).toHaveLength(1);
    expect(v[0]).toMatchObject({ line: 1, kind: 'cjk' });
  });

  it('flags fullwidth punctuation left behind after a translation sweep', () => {
    const v = scanText('a.ts', `const a = 1; // JSI ${FULLWIDTH_PAREN}\n`);
    expect(v).toHaveLength(1);
    expect(v[0].kind).toBe('cjk');
  });

  it('passes a file that is entirely English', () => {
    expect(scanText('a.ts', 'const a = 1; // plain ASCII\n')).toHaveLength(0);
  });

  it('reports every offending line, not just the first', () => {
    const v = scanText('a.ts', `// ${HAN}\nconst a = 1;\n// ${HAN}\n`);
    expect(v.map((x) => x.line)).toEqual([1, 3]);
  });
});

describe('check:cjk — line pragma', () => {
  it('exempts the line it sits on and the line after it', () => {
    const source = `// cjk-allow: sample data ${HAN}\nconst next = '${HAN}';\n`;
    expect(scanText('a.ts', source)).toHaveLength(0);
  });

  it('stops covering the line after next', () => {
    const source = `// cjk-allow: reason\nconst covered = '${HAN}';\nconst uncovered = '${HAN}';\n`;
    const v = scanText('a.ts', source);
    expect(v).toHaveLength(1);
    expect(v[0].line).toBe(3);
  });

  it('rejects a pragma with no reason', () => {
    const v = scanText('a.ts', `// cjk-allow:\nconst a = '${HAN}';\n`);
    expect(v.some((x) => x.kind === 'empty-reason' && x.line === 1)).toBe(true);
  });

  it('rejects a pragma whose reason is only whitespace, and still flags the CJK', () => {
    const v = scanText('a.ts', `// cjk-allow:   \nconst a = '${HAN}';\n`);
    expect(v.some((x) => x.kind === 'empty-reason')).toBe(true);
    expect(v.some((x) => x.kind === 'cjk' && x.line === 2)).toBe(true);
  });
});

describe('check:cjk — file pragma', () => {
  it('exempts the whole file when declared near the top', () => {
    const source = `// cjk-allow-file: this file documents CJK encoding\n${'const x = 1;\n'.repeat(40)}const a = '${HAN}';\n`;
    expect(scanText('a.ts', source)).toHaveLength(0);
  });

  it('does NOT exempt when declared past the scan window', () => {
    const source = `${'const x = 1;\n'.repeat(25)}// cjk-allow-file: too late to count\nconst a = '${HAN}';\n`;
    const v = scanText('a.ts', source);
    expect(v.some((x) => x.kind === 'cjk')).toBe(true);
  });

  it('rejects a file pragma with no reason', () => {
    const v = scanText('a.ts', `// cjk-allow-file:\nconst a = '${HAN}';\n`);
    expect(v.some((x) => x.kind === 'empty-reason' && x.line === 1)).toBe(true);
  });

  it('accepts the markdown comment form, since only the token is matched', () => {
    const source = `<!-- cjk-allow-file: Chinese quick-start kept inline -->\n# Title\n\n${HAN}\n`;
    expect(scanText('a.md', source)).toHaveLength(0);
  });
});

describe('check:cjk — bilingual markdown', () => {
  it('discounts a language switcher pointing at the .zh.md counterpart', () => {
    const source = `# Title\n\n[${HAN}](./README.zh.md)\n`;
    expect(scanText('README.md', source)).toHaveLength(0);
  });

  it('still flags real Chinese prose in an English document', () => {
    const source = `# Title\n\n[${HAN}](./README.zh.md)\n\n${HAN}${HAN} is prose, not a link.\n`;
    const v = scanText('README.md', source);
    expect(v).toHaveLength(1);
    expect(v[0].line).toBe(5);
  });

  it('does not apply the switcher discount to source files', () => {
    const source = `// [${HAN}](./README.zh.md)\n`;
    expect(scanText('a.ts', source)).toHaveLength(1);
  });
});
