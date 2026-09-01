/**
 * Sandbox compatibility helpers for guest-bundle code.
 *
 * Guest sandboxes (iOS JSC, Android Hermes, QuickJS native/wasm) provide only
 * core ECMAScript intrinsics — web platform globals like `performance` and
 * `TextEncoder` are NOT guaranteed. Everything in the guest bundle that needs
 * them must go through these call-time-checked helpers instead of touching the
 * globals directly. Checks are deliberately performed on every call (not at
 * module eval) so a host that injects the globals after bundle load is picked
 * up, and so tests can simulate a bare sandbox by deleting them.
 *
 * These helpers are GUEST-FACING: `keel/guest` re-exports them (`src/sdk/index.ts`,
 * issue #158) because a sandboxed guest has no `TextEncoder` of its own to fall
 * back on. They are therefore public surface, not internal plumbing — do not
 * retire one because its last in-tree caller went away.
 *
 * Measured inside a keel guest running on QuickJS-WASM (issue #158), reporting
 * `typeof globalThis[name]` from inside the sandbox: `TextEncoder`,
 * `TextDecoder`, `performance`, `btoa`, `atob`, `Buffer` and `crypto` are all
 * ABSENT, while `Uint8Array`, `console` and `setTimeout` are present. That
 * combination is what makes the hazard easy to miss — enough of the environment
 * works that the missing pieces read as a surprise rather than as a category —
 * and it is why every encoding helper here needs a pure-JS path, not just a
 * portability caveat.
 *
 * The two byte<->text directions are held to different standards on purpose:
 *   - ENCODE must be byte-identical to `TextEncoder.encode`, because the host's
 *     wire decoders are FATAL — one wrong byte fails the whole message closed.
 *   - DECODE must agree with a NON-FATAL `TextDecoder` (the WHATWG replacement
 *     rules), because a guest decoding host bytes wants the same lossy-but-
 *     defined result a browser would give, not an invention of ours.
 */

interface MaybePerformance {
  performance?: { now?: () => number };
}

/**
 * Monotonic-ish milliseconds: `performance.now()` when the sandbox has it,
 * `Date.now()` otherwise. Fallback resolution is ~1ms — fine for the
 * diagnostics/stats callers this is meant for; do not use for wire data.
 */
export function nowMs(): number {
  const p = (globalThis as MaybePerformance).performance;
  return p && typeof p.now === 'function' ? p.now() : Date.now();
}

/**
 * Returns a UTF-8 encode function, byte-identical to `TextEncoder.encode` —
 * including lone surrogates, which encode as U+FFFD (ef bf bd) per WHATWG.
 * Uses a single shared TextEncoder when the sandbox has one, a pure-JS
 * fallback otherwise; the wire decoders on the host side use a fatal
 * TextDecoder, so byte-exact output is a hard requirement. Call once per
 * batch/scope and reuse the returned function inside loops.
 */
export function createUtf8Encoder(): (str: string) => Uint8Array {
  const encoderCtor = (globalThis as { TextEncoder?: typeof TextEncoder }).TextEncoder;
  if (typeof encoderCtor === 'function') {
    const encoder = new encoderCtor();
    return (str) => encoder.encode(str);
  }
  return encodeUtf8Fallback;
}

function encodeUtf8Fallback(str: string): Uint8Array {
  const out: number[] = [];
  for (let i = 0; i < str.length; i++) {
    // Reason: i < str.length guarantees a code point at i
    let cp = str.codePointAt(i) as number;
    if (cp > 0xffff) {
      i++; // consumed a full surrogate pair
    } else if (cp >= 0xd800 && cp <= 0xdfff) {
      cp = 0xfffd; // lone surrogate → replacement char, matching TextEncoder
    }
    if (cp < 0x80) {
      out.push(cp);
    } else if (cp < 0x800) {
      out.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
    } else if (cp < 0x10000) {
      out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
    } else {
      out.push(
        0xf0 | (cp >> 18),
        0x80 | ((cp >> 12) & 0x3f),
        0x80 | ((cp >> 6) & 0x3f),
        0x80 | (cp & 0x3f)
      );
    }
  }
  return Uint8Array.from(out);
}

/**
 * Returns a UTF-8 decode function, output-identical to a NON-FATAL
 * `TextDecoder('utf-8').decode` — malformed input yields U+FFFD by the WHATWG
 * replacement rules rather than throwing.
 *
 * The counterpart to [`createUtf8Encoder`], and the direction that did not exist
 * before: a guest receiving bytes from a host capability (a file it asked the
 * host to read, a fetch response body) had neither a helper nor a documented
 * fallback, because keel's own wire path only ever needed to encode. Uses a
 * single shared `TextDecoder` when the sandbox has one and a pure-JS fallback
 * otherwise; like the encoder, the check is per call, so a host that injects the
 * global late is picked up. Call once per batch/scope and reuse the returned
 * function inside loops.
 *
 * Non-fatal is the right default here, and it is the opposite of the choice the
 * HOST's wire decoders make (`wire-decoder.ts` uses `fatal: true`). Those two are
 * not in tension: a malformed wire frame is a protocol violation that must fail
 * the whole message closed, while a guest decoding arbitrary bytes it was handed
 * wants the lossy-but-defined result — the same string a browser would produce.
 * A guest that needs strictness can encode the result back and compare.
 */
export function createUtf8Decoder(): (bytes: Uint8Array) => string {
  const decoderCtor = (globalThis as { TextDecoder?: typeof TextDecoder }).TextDecoder;
  if (typeof decoderCtor === 'function') {
    const decoder = new decoderCtor('utf-8');
    return (bytes) => decoder.decode(bytes);
  }
  return decodeUtf8Fallback;
}

/**
 * The WHATWG "UTF-8 decode without BOM" state machine, verbatim, because the
 * replacement behaviour is where a hand-rolled decoder goes wrong: the standard
 * emits ONE U+FFFD per maximal subpart and RE-PROCESSES the byte that ended a
 * truncated sequence, so `e0 80` is two replacements, `f0 41` is a replacement
 * followed by `A`, and an overlong or surrogate encoding is rejected by the
 * per-lead-byte continuation bounds rather than after the fact. Getting that
 * wrong desynchronises the rest of the string, not just the bad byte.
 */
function decodeUtf8Fallback(bytes: Uint8Array): string {
  // Accumulate in chunks: one giant fromCharCode(...spread) blows the argument
  // limit on a large buffer, and per-character string concat is O(n^2).
  const chunks: string[] = [];
  let units: number[] = [];
  const emit = (cp: number) => {
    if (cp > 0xffff) {
      const v = cp - 0x10000;
      units.push(0xd800 + (v >> 10), 0xdc00 + (v & 0x3ff));
    } else {
      units.push(cp);
    }
    if (units.length >= 4096) {
      chunks.push(String.fromCharCode(...units));
      units = [];
    }
  };

  let codePoint = 0;
  let bytesNeeded = 0;
  let bytesSeen = 0;
  let lowerBoundary = 0x80;
  let upperBoundary = 0xbf;

  // A leading BOM is REMOVED, because `new TextDecoder('utf-8')` removes it
  // (`ignoreBOM` defaults to false) and this function's contract is to agree with
  // that. Only the first one, and only at the very start: a U+FEFF anywhere else
  // is ordinary content and is kept.
  let start = 0;
  if (bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
    start = 3;
  }

  for (let i = start; i < bytes.length; i++) {
    // `?? 0` only satisfies `noUncheckedIndexedAccess`; `i < bytes.length` already
    // guarantees the read, so the fallback is unreachable.
    const byte = bytes[i] ?? 0;
    if (bytesNeeded === 0) {
      if (byte <= 0x7f) {
        emit(byte);
      } else if (byte >= 0xc2 && byte <= 0xdf) {
        bytesNeeded = 1;
        codePoint = byte & 0x1f;
      } else if (byte >= 0xe0 && byte <= 0xef) {
        // Tighter continuation bounds reject overlongs (E0 80..) and the
        // surrogate range (ED A0..) at the first continuation byte.
        if (byte === 0xe0) lowerBoundary = 0xa0;
        if (byte === 0xed) upperBoundary = 0x9f;
        bytesNeeded = 2;
        codePoint = byte & 0x0f;
      } else if (byte >= 0xf0 && byte <= 0xf4) {
        if (byte === 0xf0) lowerBoundary = 0x90;
        if (byte === 0xf4) upperBoundary = 0x8f;
        bytesNeeded = 3;
        codePoint = byte & 0x07;
      } else {
        // 0x80..0xC1 (continuation without a lead, or an overlong 2-byte lead)
        // and 0xF5..0xFF (beyond U+10FFFF) can never start a sequence.
        emit(0xfffd);
      }
      continue;
    }
    if (byte < lowerBoundary || byte > upperBoundary) {
      // Truncated sequence: one replacement for what was seen, then RE-PROCESS
      // this byte as a fresh start — it may itself be a valid lead or ASCII.
      codePoint = 0;
      bytesNeeded = 0;
      bytesSeen = 0;
      lowerBoundary = 0x80;
      upperBoundary = 0xbf;
      emit(0xfffd);
      i--;
      continue;
    }
    lowerBoundary = 0x80;
    upperBoundary = 0xbf;
    codePoint = (codePoint << 6) | (byte & 0x3f);
    bytesSeen++;
    if (bytesSeen === bytesNeeded) {
      emit(codePoint);
      codePoint = 0;
      bytesNeeded = 0;
      bytesSeen = 0;
    }
  }
  // End of stream mid-sequence is one final replacement.
  if (bytesNeeded !== 0) emit(0xfffd);

  if (units.length > 0) chunks.push(String.fromCharCode(...units));
  return chunks.join('');
}

// Standard base64 alphabet (RFC 4648 §4). Not base64url: this pairs with hosts
// and contracts that speak plain base64, and a guest needing the URL alphabet can
// substitute the two characters itself.
const B64_CHARS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

// Reverse lookup: input char code -> 6-bit value, or -1 for "not in the
// alphabet". Indexed by number and read with `?? -1`, so the decode loop needs
// neither a per-character `indexOf` scan nor string index access. The forward
// direction needs no table at all — `B64_CHARS.charCodeAt(v)` is the answer, and
// unlike an index expression it is typed as a plain `number`.
const B64_VALUES = new Int8Array(256).fill(-1);
for (let i = 0; i < 64; i++) {
  B64_VALUES[B64_CHARS.charCodeAt(i)] = i;
}

// ASCII whitespace, which `atob` skips anywhere in its input.
const CH_TAB = 0x09;
const CH_LF = 0x0a;
const CH_FF = 0x0c;
const CH_CR = 0x0d;
const CH_SPACE = 0x20;
const CH_EQUALS = 0x3d;

function isBase64Whitespace(code: number): boolean {
  return code === CH_SPACE || code === CH_TAB || code === CH_LF || code === CH_CR || code === CH_FF;
}

/**
 * Encode bytes as standard base64 (RFC 4648 §4, padded).
 *
 * Present because `btoa` is absent from the sandbox too — measured, not assumed
 * (see the file header). Base64 is the natural next reach for a guest that has to
 * move bytes through a text-shaped interface, so without this it is the second
 * thing every guest author re-implements after UTF-8.
 *
 * Always pure JS, deliberately, unlike the UTF-8 helpers: `btoa` takes a STRING
 * of latin-1 code units, so routing bytes through it means first building a
 * binary string — an extra copy plus a trap (any code unit above 0xFF throws).
 * The output is fully specified by the RFC, so there is no byte-exactness reason
 * to prefer the platform, and being global-free makes this immune to whether a
 * host injects one.
 */
export function encodeBase64(bytes: Uint8Array): string {
  const chunks: string[] = [];
  let units: number[] = [];
  const push = (...codes: number[]) => {
    units.push(...codes);
    if (units.length >= 4096) {
      chunks.push(String.fromCharCode(...units));
      units = [];
    }
  };

  // `?? 0` on each read only satisfies `noUncheckedIndexedAccess` — the loop bound
  // and the `rest` branch already guarantee every index is in range, so the
  // fallback is unreachable rather than a behaviour.
  const at = (index: number): number => bytes[index] ?? 0;

  let i = 0;
  for (; i + 2 < bytes.length; i += 3) {
    const n = (at(i) << 16) | (at(i + 1) << 8) | at(i + 2);
    push(
      B64_CHARS.charCodeAt((n >> 18) & 63),
      B64_CHARS.charCodeAt((n >> 12) & 63),
      B64_CHARS.charCodeAt((n >> 6) & 63),
      B64_CHARS.charCodeAt(n & 63)
    );
  }
  // Tail: 1 or 2 leftover bytes pad out to a full quantum.
  const rest = bytes.length - i;
  if (rest === 1) {
    const n = at(i) << 16;
    push(
      B64_CHARS.charCodeAt((n >> 18) & 63),
      B64_CHARS.charCodeAt((n >> 12) & 63),
      CH_EQUALS,
      CH_EQUALS
    );
  } else if (rest === 2) {
    const n = (at(i) << 16) | (at(i + 1) << 8);
    push(
      B64_CHARS.charCodeAt((n >> 18) & 63),
      B64_CHARS.charCodeAt((n >> 12) & 63),
      B64_CHARS.charCodeAt((n >> 6) & 63),
      CH_EQUALS
    );
  }

  if (units.length > 0) chunks.push(String.fromCharCode(...units));
  return chunks.join('');
}

/**
 * Decode standard base64 into bytes. Throws `Error` on input that is not valid
 * base64, matching `atob`'s fail-loud contract.
 *
 * Follows the same tolerances `atob` has, so a guest gets the same answer with or
 * without the global: ASCII whitespace is ignored anywhere, and trailing padding
 * may be omitted. Two things are still rejected rather than silently absorbed —
 * a character count that is 1 more than a multiple of 4 (no base64 quantum can
 * hold a single character, so such input is truncated or corrupt), and data after
 * the padding. Fail-loud rather than lossy is the right side here: a corrupted
 * byte payload that decodes to garbage is worse than one that stops.
 */
export function decodeBase64(text: string): Uint8Array {
  // Upper bound: 3 bytes per 4 characters. Trimmed to the exact length on return.
  const out = new Uint8Array(((text.length + 3) >> 2) * 3);
  let written = 0;
  let acc = 0;
  let accBits = 0;
  let chars = 0;
  let sawPadding = false;

  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (isBase64Whitespace(code)) continue;
    if (code === CH_EQUALS) {
      sawPadding = true;
      continue;
    }
    if (sawPadding) {
      throw new Error(`decodeBase64: data after padding at index ${i}`);
    }
    const value = code < 256 ? (B64_VALUES[code] ?? -1) : -1;
    if (value < 0) {
      throw new Error(`decodeBase64: invalid character at index ${i}`);
    }
    chars++;
    acc = (acc << 6) | value;
    accBits += 6;
    if (accBits >= 8) {
      accBits -= 8;
      out[written++] = (acc >> accBits) & 0xff;
    }
  }

  if (chars % 4 === 1) {
    throw new Error('decodeBase64: invalid length (a base64 quantum cannot hold 1 character)');
  }
  // slice, not subarray: the caller keeps the result, and a view would retain the
  // whole over-allocated buffer.
  return out.slice(0, written);
}
