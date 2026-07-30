/**
 * The byte<->text half of `rill/guest`'s sandbox-safe utilities (issue #68):
 * `createUtf8Decoder`, `encodeBase64` and `decodeBase64`.
 *
 * The decode direction's whole risk is MALFORMED input. Well-formed UTF-8 is easy
 * to get right and easy to spot-check; the WHATWG replacement rules are neither,
 * and getting them wrong desynchronises the rest of the string rather than
 * corrupting one character. So the pure-JS fallback is compared against a real
 * non-fatal `TextDecoder` EXHAUSTIVELY over every 1-byte and every 2-byte input
 * (all 65,536), not over a sample — a sampled corpus is exactly how a wrong
 * maximal-subpart rule survives review.
 *
 * Non-ASCII samples are built with String.fromCodePoint so the source stays ASCII
 * (same convention as `sandbox-utils.test.ts`), while still exercising 2-byte
 * (U+00E9), 3-byte (CJK U+4E2D, U+4F60, U+597D) and 4-byte (U+1F389, U+10FFFF)
 * encodings.
 *
 * Imports the leaf source, not the `./index` barrel: the barrel eagerly loads the
 * React-based SDK, which poisons react-test-renderer's lazy React setup in
 * sibling tests sharing bun's process (same reason as `sandbox-utils.test.ts`).
 * That the names are actually EXPORTED from `rill/guest` is enforced by
 * `check:types`; the bare-sandbox behaviour is covered in
 * `sandbox-compat-bare.test.ts`, which needs a fresh realm.
 */

import { describe, expect, it } from 'bun:test';
import {
  createUtf8Decoder,
  createUtf8Encoder,
  decodeBase64,
  encodeBase64,
} from '../guest/runtime/reconciler/sandbox-compat';

/** The pure-JS fallback, reached by hiding the global for one call. */
function fallbackDecoder(): (b: Uint8Array) => string {
  const g = globalThis as { TextDecoder?: unknown };
  const saved = g.TextDecoder;
  g.TextDecoder = undefined;
  try {
    return createUtf8Decoder();
  } finally {
    g.TextDecoder = saved;
  }
}

/** Same trick for the encoder, so a round-trip can stay entirely pure-JS. */
function fallbackEncoder(): (s: string) => Uint8Array {
  const g = globalThis as { TextEncoder?: unknown };
  const saved = g.TextEncoder;
  g.TextEncoder = undefined;
  try {
    return createUtf8Encoder();
  } finally {
    g.TextEncoder = saved;
  }
}

const reference = new TextDecoder('utf-8'); // non-fatal, per the helper's contract

const CJK_ZHONG = String.fromCodePoint(0x4e2d);
const CJK_PAIR = String.fromCodePoint(0x4f60, 0x597d);
const EMOJI = String.fromCodePoint(0x1f389);

/** Malformed shapes whose handling differs between plausible implementations. */
const TRICKY: Array<[string, number[]]> = [
  ['bare continuation byte', [0x80]],
  ['overlong 2-byte NUL', [0xc0, 0x80]],
  ['overlong 3-byte', [0xe0, 0x80, 0x80]],
  ['truncated 3-byte, then ASCII', [0xe2, 0x82, 0x41]],
  ['truncated 4-byte, then ASCII', [0xf0, 0x9f, 0x41]],
  ['lead byte then ASCII (re-processed)', [0xf0, 0x41]],
  ['surrogate encoded as 3 bytes', [0xed, 0xa0, 0x80]],
  ['beyond U+10FFFF', [0xf5, 0x80, 0x80, 0x80]],
  ['0xff', [0xff]],
  ['0xc1 (overlong lead)', [0xc1, 0xbf]],
  ['truncated at end of stream', [0xe2, 0x82]],
  ['valid then truncated', [0x41, 0xe2, 0x82]],
  ['two truncations in a row', [0xe0, 0x80, 0xe0, 0x80]],
  ['max 4-byte code point', [0xf4, 0x8f, 0xbf, 0xbf]],
  ['just past max', [0xf4, 0x90, 0x80, 0x80]],
  ['leading BOM is removed', [0xef, 0xbb, 0xbf, 0x41]],
  ['BOM alone', [0xef, 0xbb, 0xbf]],
  ['second BOM is kept as content', [0xef, 0xbb, 0xbf, 0xef, 0xbb, 0xbf, 0x41]],
  ['BOM not at the start is kept', [0x41, 0xef, 0xbb, 0xbf]],
  ['empty', []],
];

describe('createUtf8Decoder (#68)', () => {
  it('uses the platform TextDecoder when the sandbox has one', () => {
    const decode = createUtf8Decoder();
    expect(decode(new Uint8Array([0xe4, 0xb8, 0xad]))).toBe(CJK_ZHONG);
  });

  it('round-trips every code-point class through the pure-JS pair', () => {
    const encode = fallbackEncoder();
    const decode = fallbackDecoder();
    for (const s of [
      '',
      'ascii $',
      String.fromCodePoint(0x00e9),
      CJK_PAIR,
      EMOJI,
      `a${String.fromCodePoint(0x10ffff)}b`,
      `mixed ${CJK_PAIR} + emoji ${EMOJI} + ascii`,
    ]) {
      expect(decode(encode(s))).toBe(s);
    }
  });

  it.each(TRICKY)('matches a non-fatal TextDecoder: %s', (_name, bytes) => {
    const decode = fallbackDecoder();
    const input = new Uint8Array(bytes);
    expect(decode(input)).toBe(reference.decode(input));
  });

  it('matches a non-fatal TextDecoder for every single byte (256/256)', () => {
    const decode = fallbackDecoder();
    for (let b = 0; b <= 0xff; b++) {
      const input = new Uint8Array([b]);
      if (decode(input) !== reference.decode(input)) {
        throw new Error(`byte 0x${b.toString(16)} disagrees`);
      }
    }
  });

  it('matches a non-fatal TextDecoder for all 65536 two-byte inputs', () => {
    const decode = fallbackDecoder();
    const input = new Uint8Array(2);
    for (let a = 0; a <= 0xff; a++) {
      for (let b = 0; b <= 0xff; b++) {
        input[0] = a;
        input[1] = b;
        if (decode(input) !== reference.decode(input)) {
          throw new Error(`[0x${a.toString(16)}, 0x${b.toString(16)}] disagrees`);
        }
      }
    }
  });

  it('matches a non-fatal TextDecoder for every 3-byte sequence under every multi-byte lead', () => {
    const decode = fallbackDecoder();
    // Exhaustive over the two continuation slots for each lead byte that starts a
    // 3- or 4-byte sequence: 21 leads x 65,536 = ~1.4M inputs. Worth the seconds
    // -- a hand-picked grid is what let a BOM-shaped sequence (ef bb bf) slip
    // through the first version of this suite, and a leading BOM turned out to be
    // the one case where the fallback genuinely disagreed with the platform.
    const input = new Uint8Array(3);
    for (let lead = 0xe0; lead <= 0xf4; lead++) {
      input[0] = lead;
      for (let c1 = 0; c1 <= 0xff; c1++) {
        input[1] = c1;
        for (let c2 = 0; c2 <= 0xff; c2++) {
          input[2] = c2;
          if (decode(input) !== reference.decode(input)) {
            throw new Error(
              `sequence [0x${lead.toString(16)}, 0x${c1.toString(16)}, 0x${c2.toString(16)}] disagrees`
            );
          }
        }
      }
    }
  });

  it('matches a non-fatal TextDecoder when a bad sequence is followed by more input', () => {
    const decode = fallbackDecoder();
    // Desync check: a truncated sequence must not eat what comes after it. Every
    // multi-byte lead against boundary-straddling continuations, with real tails.
    const conts = [0x00, 0x41, 0x7f, 0x80, 0x8f, 0x90, 0x9f, 0xa0, 0xbb, 0xbf, 0xc0, 0xff];
    for (let lead = 0xc0; lead <= 0xff; lead++) {
      for (const c1 of conts) {
        for (const tail of [[0x41], [0xc3, 0xa9], [0xe4, 0xb8, 0xad], [0x80]]) {
          const input = new Uint8Array([lead, c1, ...tail]);
          if (decode(input) !== reference.decode(input)) {
            throw new Error(`sequence ${Array.from(input).join(',')} disagrees`);
          }
        }
      }
    }
  });

  it('handles a buffer large enough to exercise the chunked accumulator', () => {
    const decode = fallbackDecoder();
    // Past the 4096-unit flush, and with a multi-byte character straddling it.
    const s = `${'a'.repeat(5000)}${CJK_ZHONG}${'b'.repeat(5000)}`;
    const bytes = fallbackEncoder()(s);
    expect(decode(bytes)).toBe(s);
    expect(decode(bytes)).toBe(reference.decode(bytes));
  });
});

describe('base64 (#68)', () => {
  const SAMPLES: number[][] = [
    [],
    [0x00],
    [0xff],
    [0x00, 0xff],
    [0x00, 0xff, 0x7f],
    [0x66, 0x6f, 0x6f, 0x62, 0x61, 0x72],
    Array.from({ length: 256 }, (_, i) => i),
    // Every tail length, so both padding branches are covered.
    [1],
    [1, 2],
    [1, 2, 3],
    [1, 2, 3, 4],
    [1, 2, 3, 4, 5],
  ];

  it('agrees with btoa on every sample', () => {
    for (const bytes of SAMPLES) {
      const expected = btoa(String.fromCharCode(...bytes));
      expect(encodeBase64(new Uint8Array(bytes))).toBe(expected);
    }
  });

  it('round-trips every sample', () => {
    for (const bytes of SAMPLES) {
      const encoded = encodeBase64(new Uint8Array(bytes));
      expect(Array.from(decodeBase64(encoded))).toEqual(bytes);
    }
  });

  it('agrees with atob on decoding', () => {
    for (const bytes of SAMPLES) {
      const encoded = encodeBase64(new Uint8Array(bytes));
      const viaAtob = Array.from(atob(encoded), (c) => c.charCodeAt(0));
      expect(Array.from(decodeBase64(encoded))).toEqual(viaAtob);
    }
  });

  it('tolerates what atob tolerates: whitespace and missing padding', () => {
    expect(Array.from(decodeBase64('Zm9v'))).toEqual([0x66, 0x6f, 0x6f]);
    expect(Array.from(decodeBase64('Zm 9\nv'))).toEqual([0x66, 0x6f, 0x6f]);
    // "Zg==" / "Zg" both decode to one byte; "Zm8=" / "Zm8" to two.
    expect(Array.from(decodeBase64('Zg=='))).toEqual([0x66]);
    expect(Array.from(decodeBase64('Zg'))).toEqual([0x66]);
    expect(Array.from(decodeBase64('Zm8='))).toEqual([0x66, 0x6f]);
    expect(Array.from(decodeBase64('Zm8'))).toEqual([0x66, 0x6f]);
  });

  it('fails loud on input that is not base64', () => {
    // A lone character cannot be a quantum — atob throws here too.
    expect(() => decodeBase64('Z')).toThrow(/invalid length/);
    expect(() => decodeBase64('Zm9!')).toThrow(/invalid character/);
    expect(() => decodeBase64('Zm9v!')).toThrow(/invalid character/);
    // 5 VALID characters cannot form quanta at all -- the length rule, which is
    // checked after the scan, so an invalid character is reported first.
    expect(() => decodeBase64('ZmZm9')).toThrow(/invalid length/);
    // Data after padding is corruption, not tolerance (atob rejects it too).
    expect(() => decodeBase64('Zg==Zg==')).toThrow(/data after padding/);
    // base64url is deliberately NOT accepted: its two extra characters would
    // silently decode to different bytes than the sender meant.
    expect(() => decodeBase64('a-b_')).toThrow(/invalid character/);
  });

  it('carries a UTF-8 payload through base64 unchanged', () => {
    const encode = fallbackEncoder();
    const decode = fallbackDecoder();
    const s = `bytes through a text-shaped interface: ${CJK_PAIR} ${EMOJI}`;
    expect(decode(decodeBase64(encodeBase64(encode(s))))).toBe(s);
  });
});
