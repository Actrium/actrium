/**
 * Bare-sandbox probe — run as a SUBPROCESS by `sandbox-compat-bare.test.ts`
 * (issue #68). NOT a test file itself: no `.test.ts` suffix, so `bun test` does
 * not collect it, and it must stay that way.
 *
 * The point of a separate process is the fresh realm. The globals are deleted
 * BEFORE `sandbox-compat` is imported, which reproduces what a real sandbox looks
 * like — absent from the start, not removed mid-run — and covers module-eval time
 * as well as call time. Doing this inside the shared `bun test` process instead
 * would leak the deletions into every other test file in the run.
 *
 * Measured on QuickJS-WASM, all seven of these are absent from a real keel guest
 * sandbox (see `sandbox-compat.ts`'s header), so this is the environment the
 * helpers actually ship into.
 */

export {}; // top-level await needs this file to be a module

const DELETED = ['TextEncoder', 'TextDecoder', 'performance', 'btoa', 'atob', 'Buffer', 'crypto'];

const g = globalThis as unknown as Record<string, unknown>;
for (const name of DELETED) {
  delete g[name];
}

// Import only AFTER the deletions, so module evaluation itself has to survive a
// bare sandbox. A helper that captured a global at import time would fail here.
const { createUtf8Encoder, createUtf8Decoder, nowMs, encodeBase64, decodeBase64 } = await import(
  './sandbox-compat'
);

const stillPresent = DELETED.filter((name) => typeof g[name] !== 'undefined');

// Samples built from code points so this file stays ASCII: 1-, 2-, 3- and 4-byte
// sequences plus a lone surrogate (which must encode as U+FFFD).
const samples = [
  '',
  'ascii $',
  String.fromCodePoint(0x00e9),
  String.fromCodePoint(0x4f60, 0x597d),
  String.fromCodePoint(0x1f389),
  `a${String.fromCodePoint(0x10ffff)}b`,
  `a${String.fromCharCode(0xd800)}b`,
];

const encode = createUtf8Encoder();
const decode = createUtf8Decoder();

const encoded = samples.map((s) => Array.from(encode(s)));
// Round-trip: every sample except the lone surrogate must come back identical
// (the surrogate legitimately becomes U+FFFD on the way out).
const roundTripped = samples.map((s) => decode(encode(s)));
const base64 = samples.map((s) => encodeBase64(encode(s)));
const base64RoundTripped = base64.map((b) => decode(decodeBase64(b)));

// A payload spanning all four sequence lengths, as bytes, for a length assertion
// the caller can check independently of string equality.
const mixed = `a${String.fromCodePoint(0x00e9)}${String.fromCodePoint(0x4e2d)}${String.fromCodePoint(0x1f389)}`;

const result = {
  stillPresent,
  encoded,
  roundTripped,
  base64,
  base64RoundTripped,
  mixedByteLength: encode(mixed).length,
  loneSurrogateBytes: Array.from(encode(String.fromCharCode(0xd800))),
  // nowMs must fall back to Date.now() rather than throwing on absent performance.
  nowMsType: typeof nowMs(),
  nowMsPositive: nowMs() > 0,
  // Malformed input still follows the WHATWG replacement rules with no TextDecoder.
  malformed: decode(new Uint8Array([0xe0, 0x80, 0x41])),
  bomStripped: decode(new Uint8Array([0xef, 0xbb, 0xbf, 0x41])),
};

console.log(JSON.stringify(result));
