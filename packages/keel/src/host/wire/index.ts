/**
 * keel/wire — zero-DOM binary wire decoders (public export surface, R1.3).
 *
 * This is the importable home for the guest→host binary wire decoders. Both
 * decoders here are STRICTLY ZERO-DOM: they reference no `document`,
 * `HTMLCanvasElement`, `CanvasRenderingContext2D`, `window`, or any other
 * browser/DOM type. They turn raw bytes into plain, JSON-shaped data (an op
 * array / a stream of operations); the host replays that data onto its own
 * surfaces. This lets the platform bundle the decoders anywhere — a worker,
 * node, an edge runtime — without special handling.
 *
 * Two sister wires, deliberately distinct (different magic, so a buffer meant
 * for one decoder fails the other's u32 magic compare immediately):
 *   - op-batch  (magic 'KEEL') — UI-tree diffs over a retained node graph.
 *   - canvas    (magic 'KCNV') — a flat, per-frame 2D display list.
 *
 * Platform consumers import from here, e.g.:
 *   import { decodeCanvasBatch } from 'keel/wire';
 *
 * WIP / EXPERIMENTAL: neither decoder is on a live receive path yet; they ship
 * off, locked to the contracts + golden vectors under contracts/sandbox/.
 */

// --- canvas wire (contracts/sandbox/canvas-wire.json) ---
export {
  type CanvasBatchHeader,
  CanvasDecodeError,
  type CanvasDecodeReason,
  type CanvasOp,
  type CanvasWireInput,
  decodeCanvasBatch,
  decodeCanvasBatchStreaming,
  decodeCanvasFrame,
  peekCanvasHeader,
} from './canvas-wire-decoder';
// --- store/net binary-value ENVELOPE (contracts/sandbox/store-net-bytes.json) ---
// The KBS1 two-plane frame (JSON control plane + length-prefixed binary data
// plane). Unlike the two decoders above, this codec IS on a live receive path:
// NativeGuestHost forks on the KBS1 magic to decode a byte-carrying request and
// encodes a byte-carrying return. It is contract-agnostic (sentinel-driven).
export {
  type DecodedEnvelope,
  decodeEnvelope,
  decodeRequest as decodeKbs1Request,
  encodeEnvelope,
  encodeResult as encodeKbs1Result,
  type Hoisted,
  hoistSentinels,
  isKbs1,
  KBS1_MAGIC,
  LIMITS as STORE_NET_LIMITS,
  reviveSentinels,
  SENTINEL_KEY,
  StoreNetEnvelopeError,
  type StoreNetReason,
} from './store-net-envelope';
// --- op-batch wire (contracts/sandbox/op-batch-wire.json) ---
export {
  type ApplyOp,
  decodeBatchStreaming,
  type WireBatchHeader,
  WireDecodeError,
} from './wire-decoder';
