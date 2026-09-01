/**
 * `keel/guest` re-exports the sandbox-safe utilities (issue #158): `createUtf8Encoder`
 * and `nowMs`, which existed internally (guest sandboxes provide only core
 * ECMAScript -- `TextEncoder` / `performance` are not guaranteed) but were not
 * reachable from the guest entry. This asserts the guest entry now exposes them
 * and that the encoder is byte-exact, including the lone-surrogate -> U+FFFD rule
 * the host's fatal wire decoders depend on.
 *
 * Non-ASCII samples are built with String.fromCodePoint / fromCharCode so the
 * source stays ASCII while exercising 2-byte (U+00E9), 3-byte (CJK U+4F60
 * U+597D), 4-byte (emoji U+1F600, max U+10FFFF) and lone-surrogate encodings.
 *
 * Note: #158 only RE-EXPORTS these helpers; the pure-JS fallback taken when
 * `TextEncoder` is absent is unchanged and lives in sandbox-compat. A bare-
 * sandbox regression guard (globals deleted) belongs with sandbox-compat's own
 * tests in an isolated realm -- deleting globals here would leak across bun's
 * shared test process -- and is tracked as a follow-up on the issue.
 */

import { describe, expect, it } from 'bun:test';
// Import from the leaf source, NOT the `./index` barrel: the barrel eagerly
// loads the React-based SDK, which poisons react-test-renderer's carefully
// ordered lazy React setup in sibling tests sharing bun's process. The #158 fix
// itself -- `export { createUtf8Encoder, nowMs }` from `src/sdk/index.ts` -- is a
// pure forward whose validity (the names exist and are exported) is enforced by
// `check:types` (tsc); this file verifies the helpers' runtime behavior.
import { createUtf8Encoder, nowMs } from '../guest/runtime/reconciler/sandbox-compat';

const SAMPLES = [
  '',
  'ascii $',
  String.fromCodePoint(0x00e9), // e-acute (2-byte)
  String.fromCodePoint(0x4f60, 0x597d), // CJK pair (3-byte each)
  String.fromCodePoint(0x1f600), // emoji (4-byte)
  `a${String.fromCodePoint(0x10ffff)}b`, // max code point
  `a${String.fromCharCode(0xd800)}b`, // lone high surrogate -> U+FFFD
];

describe('keel/guest sandbox utilities (#158)', () => {
  it('re-exports createUtf8Encoder and nowMs from the guest entry', () => {
    expect(typeof createUtf8Encoder).toBe('function');
    expect(typeof nowMs).toBe('function');
    expect(typeof nowMs()).toBe('number');
    expect(nowMs()).toBeGreaterThan(0);
  });

  it('createUtf8Encoder is byte-identical to TextEncoder (incl. lone surrogate -> U+FFFD)', () => {
    const encode = createUtf8Encoder();
    const ref = new TextEncoder();
    for (const s of SAMPLES) {
      expect(Array.from(encode(s))).toEqual(Array.from(ref.encode(s)));
    }
    // Pin the lone-surrogate replacement explicitly.
    expect(Array.from(encode(String.fromCharCode(0xd800)))).toEqual([0xef, 0xbf, 0xbd]);
  });
});
