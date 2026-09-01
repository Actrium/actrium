/**
 * store-net-envelope.ts
 *
 * Host-side codec for the KBS1 binary-value ENVELOPE (the R2 first-class-bytes
 * wire). The authoritative wire schema is contracts/sandbox/store-net-bytes.json; the
 * byte-exact golden oracle is contracts/sandbox/store-net-bytes.golden.json (produced by
 * the Rust encoder crates/keel-native-guest/src/store_net_encode.rs). This module is
 * validated against those files and MUST NOT hand-copy any peer codec.
 *
 * ZERO-DOM: references no `document`, `HTMLCanvasElement`, `window`, or any other
 * DOM/browser type. It moves bytes between the WASM guest and the host:*
 * dispatch, contract-agnostically — the wire is SELF-DESCRIBING via `{"$b":N}`
 * sentinels, so no per-call descriptor is consulted here.
 *
 * Unlike the op-batch/canvas decoders (opcode streams), KBS1 is a generic
 * two-plane frame: a JSON CONTROL PLANE plus a length-prefixed BINARY DATA PLANE.
 * The same framing is used in BOTH directions — a guest->host request and a
 * host->guest return — so there is one codec, exercised symmetrically.
 *
 * BACK-COMPAT (load-bearing): a call/return with NO byte streams is NEVER an
 * KBS1 frame — it is the plain raw-JSON body, byte-for-byte identical to today.
 * `encodeResult` returns `null` for such a result so the caller emits the
 * unchanged `JSON.stringify` reply; `decodeRequest` is only reached when the
 * magic already matched.
 *
 * FAIL-CLOSED: every `u32` length is validated against BOTH its cap AND the
 * remaining buffer before it is used; on any breach the whole frame is dropped
 * with a typed reason (never a partial value, never an out-of-bounds read).
 */

// ============================================
// Wire constants (locked to contracts/sandbox/store-net-bytes.json)
// ============================================

// magic.u32le — 0x3153424b = 'KBS1' (bytes 4B 42 53 31). DISTINCT from op-batch's
// 0x4c45454b ('KEEL') and canvas's 0x564e434b ('KCNV'): all three share byte 0
// (0x4B 'K') but differ at byte 1, so a full-u32 magic compare routes a frame to
// exactly one decoder. The 4th byte ('1') carries the wire version.
export const KBS1_MAGIC = 0x3153424b;

// The magic bytes in wire (little-endian) order, for a byte-wise peek that never
// allocates a DataView.
const MAGIC_B0 = 0x4b; // 'K'
const MAGIC_B1 = 0x42; // 'B'
const MAGIC_B2 = 0x53; // 'S'
const MAGIC_B3 = 0x31; // '1'

// Fail-closed caps (single-sourced in the schema; a conformance test locks these
// TS literals to it exactly as the Rust codec is generated from it).
export const LIMITS = {
  maxSegments: 16,
  maxSegmentBytes: 1024 * 1024, // 1 MiB
  // 16 MiB of segment payload + 256 KiB of bounded control-plane headroom.
  maxEnvelopeBytes: 16 * 1024 * 1024 + 256 * 1024, // 16.25 MiB
  maxJsonBytes: 256 * 1024, // 256 KiB
} as const;

/** The reserved sentinel key: `{"$b": N}` marks a byte-stream field. */
export const SENTINEL_KEY = '$b';

/** The stable fail-closed reason vocabulary (schema `reasons`). */
export type StoreNetReason =
  | 'bad-magic'
  | 'truncated'
  | 'json-too-big'
  | 'bad-json'
  | 'too-many-segments'
  | 'segment-too-big'
  | 'envelope-too-big'
  | 'bad-segment-ref'
  | 'bad-sentinel';

/** A typed, fail-closed envelope codec error carrying one schema reason token. */
export class StoreNetEnvelopeError extends Error {
  readonly reason: StoreNetReason;
  constructor(reason: StoreNetReason, detail?: string) {
    super(detail ? `KBS1 ${reason}: ${detail}` : `KBS1 ${reason}`);
    this.name = 'StoreNetEnvelopeError';
    this.reason = reason;
  }
}

// ============================================
// Magic peek (the host fork's routing primitive)
// ============================================

/** Whether `raw` begins with the full 4-byte KBS1 magic (compares all 4 bytes). */
export function isKbs1(raw: Uint8Array): boolean {
  return (
    raw.length >= 4 &&
    raw[0] === MAGIC_B0 &&
    raw[1] === MAGIC_B1 &&
    raw[2] === MAGIC_B2 &&
    raw[3] === MAGIC_B3
  );
}

// ============================================
// Framing primitives
// ============================================

/**
 * Frame a JSON control plane (already holding `{"$b":N}` sentinels) plus its
 * `segments` into KBS1 wire bytes. Enforces every cap; throws
 * `StoreNetEnvelopeError` (fail-closed) on any breach, emitting nothing.
 */
export function encodeEnvelope(jsonControl: string, segments: Uint8Array[]): Uint8Array {
  const json = new TextEncoder().encode(jsonControl);
  if (json.length > LIMITS.maxJsonBytes) {
    throw new StoreNetEnvelopeError('json-too-big', `${json.length} > ${LIMITS.maxJsonBytes}`);
  }
  if (segments.length > LIMITS.maxSegments) {
    throw new StoreNetEnvelopeError(
      'too-many-segments',
      `${segments.length} > ${LIMITS.maxSegments}`
    );
  }
  // magic(4) + jsonLen(4) + json + segCount(4) + sum(segLen(4) + seg)
  let total = 12 + json.length;
  for (const seg of segments) {
    if (seg.length > LIMITS.maxSegmentBytes) {
      throw new StoreNetEnvelopeError(
        'segment-too-big',
        `${seg.length} > ${LIMITS.maxSegmentBytes}`
      );
    }
    total += 4 + seg.length;
  }
  if (total > LIMITS.maxEnvelopeBytes) {
    throw new StoreNetEnvelopeError('envelope-too-big', `${total} > ${LIMITS.maxEnvelopeBytes}`);
  }

  const out = new Uint8Array(total);
  const view = new DataView(out.buffer);
  let pos = 0;
  view.setUint32(pos, KBS1_MAGIC, true);
  pos += 4;
  view.setUint32(pos, json.length, true);
  pos += 4;
  out.set(json, pos);
  pos += json.length;
  view.setUint32(pos, segments.length, true);
  pos += 4;
  for (const seg of segments) {
    view.setUint32(pos, seg.length, true);
    pos += 4;
    out.set(seg, pos);
    pos += seg.length;
  }
  return out;
}

/** A decoded envelope: the JSON control-plane bytes + each segment's bytes. */
export interface DecodedEnvelope {
  json: Uint8Array;
  segments: Uint8Array[];
}

/**
 * Unframe KBS1 wire bytes into the JSON control-plane slice + segment slices
 * (views over `buf`, zero-copy). Fail-closed: throws `StoreNetEnvelopeError` on
 * a bad magic (the caller's signal to treat `buf` as plain JSON), any length
 * overrun/over-cap, or trailing bytes (strict full consumption).
 */
export function decodeEnvelope(buf: Uint8Array): DecodedEnvelope {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  let pos = 0;
  const readU32 = (): number => {
    if (pos + 4 > buf.length) throw new StoreNetEnvelopeError('truncated', `u32 at ${pos}`);
    const v = view.getUint32(pos, true);
    pos += 4;
    return v;
  };
  const readSlice = (len: number): Uint8Array => {
    if (pos + len > buf.length)
      throw new StoreNetEnvelopeError('truncated', `${len} bytes at ${pos}`);
    const s = buf.subarray(pos, pos + len);
    pos += len;
    return s;
  };

  const magic = readU32();
  if (magic !== KBS1_MAGIC) {
    throw new StoreNetEnvelopeError('bad-magic', `0x${magic.toString(16)}`);
  }
  if (buf.length > LIMITS.maxEnvelopeBytes) {
    throw new StoreNetEnvelopeError(
      'envelope-too-big',
      `${buf.length} > ${LIMITS.maxEnvelopeBytes}`
    );
  }

  const jsonLen = readU32();
  if (jsonLen > LIMITS.maxJsonBytes) {
    throw new StoreNetEnvelopeError('json-too-big', `${jsonLen} > ${LIMITS.maxJsonBytes}`);
  }
  const json = readSlice(jsonLen);

  const segCount = readU32();
  if (segCount > LIMITS.maxSegments) {
    throw new StoreNetEnvelopeError('too-many-segments', `${segCount} > ${LIMITS.maxSegments}`);
  }
  const segments: Uint8Array[] = [];
  for (let i = 0; i < segCount; i++) {
    const segLen = readU32();
    if (segLen > LIMITS.maxSegmentBytes) {
      throw new StoreNetEnvelopeError('segment-too-big', `${segLen} > ${LIMITS.maxSegmentBytes}`);
    }
    segments.push(readSlice(segLen));
  }
  if (pos !== buf.length) {
    throw new StoreNetEnvelopeError('truncated', `${buf.length - pos} trailing bytes`);
  }
  return { json, segments };
}

// ============================================
// Sentinel walking (self-describing hoist / revive)
// ============================================

/**
 * True when `v` carries the internal slot the intrinsic `valueOf` of a
 * Number/String/Boolean wrapper demands: the intrinsic throws a TypeError on
 * any receiver without the matching [[NumberData]]/[[StringData]]/
 * [[BooleanData]] slot, and reads the slot (no user code) when present.
 */
// Reason: probes an arbitrary caller object for a wrapper internal slot; the intrinsic's receiver/return are untyped by nature.
function hasBrand(v: object, intrinsicValueOf: (this: unknown) => unknown): boolean {
  try {
    intrinsicValueOf.call(v);
    return true;
  } catch {
    return false;
  }
}

/** The result of hoisting: a JSON-serializable control value + its segments. */
export interface Hoisted {
  // Reason: the control plane is arbitrary JSON-serializable data, walked structurally.
  control: unknown;
  segments: Uint8Array[];
}

/**
 * Walk `value`, replacing every `Uint8Array` with a `{"$b":N}` sentinel (in walk
 * order) and collecting the raw bytes into `segments`. Returns a plain,
 * JSON-serializable control value. Non-`Uint8Array` data passes through
 * structurally unchanged. Byte-stream caps are enforced here (fail-closed).
 */
// Reason: walks arbitrary caller data; Uint8Array leaves hoist to segments, other values pass through.
export function hoistSentinels(value: unknown): Hoisted {
  const segments: Uint8Array[] = [];
  // `$b` is the RESERVED sentinel key. An app object `{"$b":N}` is byte-identical
  // on the wire to a Bytes sentinel, so the peer's revive could not tell them
  // apart. But that only matters when an envelope is actually framed (segments
  // present); a segment-free result takes the plain-JSON path, is never revived,
  // and MUST ride through unchanged (the back-compat invariant). So record use of
  // the key during the walk and reject AFTER, only if bytes were also hoisted.
  let sawReservedKey = false;
  // Reason: walks arbitrary caller data structurally; Uint8Array leaves hoist to
  // segments, other values pass through unchanged.
  const walk = (v: unknown, key: string): unknown => {
    // Apply JSON.stringify's toJSON hook FIRST (mirrors the spec's
    // SerializeJSONProperty step 2). The structural copy below only carries own
    // enumerable DATA props, so a Date/URL/custom object — whose JSON form lives
    // entirely in toJSON — would otherwise collapse to an empty `{}` and lose its
    // value. Call toJSON once with the SAME key JSON.stringify would pass (the
    // property name for an object member, the index string for an array element,
    // '' at the root), then serialize the returned value (its own props are
    // walked below and may themselves have toJSON). The probe includes functions:
    // per spec a callable value is still an Object, so a function carrying toJSON
    // serializes through it. Uint8Array has no toJSON, so byte leaves skip this
    // and still hoist just below; a toJSON that yields bytes is likewise hoisted.
    // Reason: probing an arbitrary caller value for JSON.stringify's toJSON hook.
    const toJSONHook =
      v !== null && (typeof v === 'object' || typeof v === 'function')
        ? (v as { toJSON?: unknown }).toJSON
        : undefined;
    if (typeof toJSONHook === 'function') {
      // Reason: caller-provided toJSON returns arbitrary JSON-shaped data.
      v = (toJSONHook as (k: string) => unknown).call(v, key);
    }
    // Unwrap Number/String/Boolean wrapper objects (spec steps 4-6): the
    // structural copy below would otherwise turn `new Number(5)` into `{}`
    // where JSON.stringify emits `5`. Brand-check by internal slot — the
    // intrinsic valueOf throws on anything without the matching [[*Data]]
    // slot — NOT instanceof, which false-positives on
    // Object.create(Number.prototype) (and would then throw even on a
    // byte-free result the plain-JSON path must pass through untouched) and
    // misses cross-realm / null-prototype wrappers. Conversions per spec:
    // ToNumber for Number, ToString for String, the [[BooleanData]] slot
    // itself for Boolean (never user code). The typeof guard is required:
    // the intrinsic valueOf also accepts same-type PRIMITIVES without
    // throwing, and those must pass through unchanged.
    if (v !== null && typeof v === 'object') {
      if (hasBrand(v, Number.prototype.valueOf)) {
        v = Number(v);
      } else if (hasBrand(v, String.prototype.valueOf)) {
        v = String(v);
      } else if (hasBrand(v, Boolean.prototype.valueOf)) {
        v = Boolean.prototype.valueOf.call(v);
      } else if (hasBrand(v, BigInt.prototype.valueOf)) {
        // The bigint primitive rides through to the final stringify, which
        // throws — or consults a user BigInt.prototype.toJSON — per spec.
        v = BigInt.prototype.valueOf.call(v);
      }
    }
    if (v instanceof Uint8Array) {
      // ONE value, possibly MANY segments. `maxSegmentBytes` caps a single
      // SEGMENT — one decoder allocation — not a logical value, so a longer value
      // is split into chunks named by the array sentinel form, keeping each
      // allocation bounded while lifting the ceiling on the value (issue #164).
      //
      // A value that fits in one segment MUST still emit the scalar form: that
      // keeps every frame expressible before this extension byte-for-byte
      // identical (so the pre-existing golden vectors lock unchanged), and never
      // hands a peer predating the array form a shape it would reject for a value
      // it could already carry.
      const chunkCount = Math.max(1, Math.ceil(v.length / LIMITS.maxSegmentBytes));
      // Check the WHOLE value against the remaining budget before pushing
      // anything, so an over-budget value leaves no partial hoist behind.
      if (chunkCount > LIMITS.maxSegments - segments.length) {
        throw new StoreNetEnvelopeError(
          'too-many-segments',
          `${segments.length} + ${chunkCount} > ${LIMITS.maxSegments}`
        );
      }
      const first = segments.length;
      if (v.length === 0) {
        // An empty value is ONE empty segment, not zero: the field must still
        // revive to an empty byte stream rather than vanish.
        segments.push(v);
      } else {
        for (let off = 0; off < v.length; off += LIMITS.maxSegmentBytes) {
          segments.push(v.subarray(off, Math.min(off + LIMITS.maxSegmentBytes, v.length)));
        }
      }
      if (segments.length - first === 1) {
        return { [SENTINEL_KEY]: first };
      }
      const indices: number[] = [];
      for (let n = first; n < segments.length; n++) {
        indices.push(n);
      }
      return { [SENTINEL_KEY]: indices };
    }
    if (Array.isArray(v)) {
      // A still-callable element (a function with no effective toJSON) becomes
      // null, as JSON.stringify does for unserializable array elements.
      return v.map((el, i) => {
        const walked = walk(el, String(i));
        return typeof walked === 'function' ? null : walked;
      });
    }
    if (v !== null && typeof v === 'object') {
      // Our own sentinels are the returned `{[SENTINEL_KEY]:n}` objects above;
      // they are never re-walked, so this only ever sees caller data.
      if (Object.hasOwn(v as Record<string, unknown>, SENTINEL_KEY)) {
        sawReservedKey = true;
      }
      const out: Record<string, unknown> = {};
      for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
        const walked = walk(val, k);
        // A still-callable member (a function with no effective toJSON) is
        // dropped, as JSON.stringify does. This must happen HERE, not be left
        // to the final stringify of the control value: a callable member named
        // `toJSON` (e.g. inside a toJSON RESULT, which the spec serializes
        // structurally without re-consulting hooks) would otherwise be
        // re-invoked as a hook by that final stringify.
        if (typeof walked === 'function') {
          continue;
        }
        // defineProperty, NOT plain assignment — the outbound twin of the
        // reviveSentinels hardening: a result carrying an own '__proto__' key
        // (e.g. data round-tripped through JSON.parse) would otherwise SET
        // the rebuilt object's prototype, silently DROPPING the field from
        // the encoded control JSON. defineProperty always creates an own
        // data property, so every key survives encoding.
        Object.defineProperty(out, k, {
          value: walked,
          enumerable: true,
          writable: true,
          configurable: true,
        });
      }
      return out;
    }
    return v;
  };
  const control = walk(value, '');
  if (segments.length > 0 && sawReservedKey) {
    throw new StoreNetEnvelopeError(
      'bad-sentinel',
      `reserved key ${SENTINEL_KEY} in a byte-carrying value`
    );
  }
  return { control, segments };
}

/**
 * Walk a parsed control value, replacing every `{"$b":N}` sentinel with
 * `segments[N]` (a `Uint8Array` view). Fail-closed on a malformed sentinel shape
 * (`bad-sentinel`) or an out-of-range index (`bad-segment-ref`).
 */
// Reason: revives a parsed control value; {"$b":N} sentinels become Uint8Array segment views.
export function reviveSentinels(value: unknown, segments: Uint8Array[]): unknown {
  // One claim per segment. The contract has always said a segment may be
  // referenced at most once; with only the scalar form that was a well-formedness
  // note, but the array form makes it load-bearing — without it a frame could name
  // one at-cap segment `maxSegments` times inside a single sentinel and force a
  // joined value far larger than the frame carrying it, breaking the very bound
  // that makes a multi-segment value safe without a new cap.
  const claimed = new Set<number>();
  const claim = (n: number): Uint8Array => {
    if (n >= segments.length) {
      throw new StoreNetEnvelopeError('bad-segment-ref', `${n} >= ${segments.length}`);
    }
    if (claimed.has(n)) {
      throw new StoreNetEnvelopeError('bad-segment-ref', `segment ${n} referenced more than once`);
    }
    claimed.add(n);
    // Reason: index checked against segments.length immediately above.
    return segments[n] as Uint8Array;
  };

  // Reason: walks a parsed control value structurally; sentinels become Uint8Array
  // segment views and every other node passes through unchanged.
  const walk = (v: unknown): unknown => {
    if (Array.isArray(v)) {
      return v.map(walk);
    }
    if (v !== null && typeof v === 'object') {
      const obj = v as Record<string, unknown>;
      if (Object.hasOwn(obj, SENTINEL_KEY)) {
        // Any object carrying the reserved key MUST be exactly `{"$b": <int>}` or
        // `{"$b": [<int>, …]}`.
        const keys = Object.keys(obj);
        const ref = obj[SENTINEL_KEY];
        if (keys.length !== 1) {
          throw new StoreNetEnvelopeError('bad-sentinel', `keys=${keys.join(',')}`);
        }
        // Reason: narrows an arbitrary parsed-JSON value under the reserved key to a
        // segment index; every non-index shape is rejected as bad-sentinel below.
        const isIndex = (x: unknown): x is number =>
          typeof x === 'number' && Number.isInteger(x) && x >= 0;
        if (isIndex(ref)) {
          return claim(ref);
        }
        // One logical value spanning several segments, joined in list order. An
        // empty list carries no bytes and no meaning, so it is malformed rather
        // than "empty value" — an empty value is one empty segment.
        if (Array.isArray(ref) && ref.length > 0 && ref.every(isIndex)) {
          // Resolve and total before allocating, so a bad reference late in the
          // list cannot leave a partly built value behind. The total needs no cap
          // of its own: the frame was already checked against maxEnvelopeBytes,
          // which bounds the sum of the segments it carries — given each is
          // claimed at most once.
          const parts = ref.map(claim);
          let total = 0;
          for (const part of parts) {
            total += part.length;
          }
          const joined = new Uint8Array(total);
          let off = 0;
          for (const part of parts) {
            joined.set(part, off);
            off += part.length;
          }
          return joined;
        }
        throw new StoreNetEnvelopeError('bad-sentinel', `${SENTINEL_KEY}=${JSON.stringify(ref)}`);
      }
      const out: Record<string, unknown> = {};
      for (const [k, val] of Object.entries(obj)) {
        // defineProperty, NOT plain assignment: the source keys are
        // guest-controlled (JSON.parse makes '__proto__' an own property), and
        // `out['__proto__'] = x` would SET the rebuilt object's prototype —
        // letting a hostile envelope inject inherited fields into the args
        // handed to host capability impls. defineProperty always creates an
        // own data property, for every key.
        Object.defineProperty(out, k, {
          value: walk(val),
          enumerable: true,
          writable: true,
          configurable: true,
        });
      }
      return out;
    }
    return v;
  };
  return walk(value);
}

// ============================================
// Top-level host helpers (the fork uses these)
// ============================================

/**
 * Decode a guest->host KBS1 request into contract-agnostic args: unframe, parse
 * the control-plane JSON, and revive each `{"$b":N}` into a `Uint8Array`. Throws
 * `StoreNetEnvelopeError` (fail-closed) on any malformed frame — the caller turns
 * that into an `ok=0` result. Only call this once `isKbs1(raw)` matched.
 */
// Reason: returns contract-agnostic args parsed from the KBS1 control-plane JSON.
export function decodeRequest(raw: Uint8Array): unknown {
  const { json, segments } = decodeEnvelope(raw);
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder().decode(json));
  } catch {
    throw new StoreNetEnvelopeError('bad-json');
  }
  return reviveSentinels(parsed, segments);
}

/**
 * Encode a host->guest return. If `result` contains at least one `Uint8Array`,
 * returns the framed KBS1 envelope; otherwise returns `null` so the caller emits
 * the IDENTICAL `JSON.stringify` reply as today (the back-compat invariant — no
 * behaviour change for a non-binary result). Throws `StoreNetEnvelopeError`
 * (fail-closed) if a binary result breaches a cap.
 */
// Reason: hoists any Uint8Array in an arbitrary host result into a KBS1 frame.
export function encodeResult(result: unknown): Uint8Array | null {
  const { control, segments } = hoistSentinels(result);
  if (segments.length === 0) {
    return null; // no byte streams: plain JSON reply, byte-for-byte unchanged
  }
  return encodeEnvelope(JSON.stringify(control), segments);
}
