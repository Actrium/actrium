import SparkMD5 from 'spark-md5';

import { ACTR_BRIDGE_CONTRACT, REALM_SERVICE_CONTRACT } from './coreContract';

/**
 * Host-side attachment upload pipeline for the fixed
 * `host:service.uploadAttachment` transport.
 *
 * The Guest hands over `{ filename, base64, type?, id? }`; the Host owns the
 * protocol: it generates the chunk `streamId` and the `resultStreamId`,
 * registers the result waiter first, prepares the native upload with both
 * ids, streams 48 KiB chunks serially (the final data chunk carries the EOS
 * marker itself; only an empty file sends a single empty EOS chunk) and waits
 * for the terminal result event. Prepare/chunk/unregister plumbing stays
 * Host-internal and is never exposed as a Guest method.
 *
 * Wire constants come from the generated `actriumCoreContract` so the TS Host
 * and the Rust/Swift core stay on one contract. The native side is the
 * per-tab `RealmService` module (`prepareAttachmentUpload`,
 * `sendAttachmentChunk`, `unregisterStream`) broadcasting one
 * `RealmServiceAttachmentResult` event.
 */

const ATTACHMENT_CONTRACT = ACTR_BRIDGE_CONTRACT.attachment;
const BRIDGE_KEYS = ACTR_BRIDGE_CONTRACT.keys;
const STREAM_METADATA = ACTR_BRIDGE_CONTRACT.streamMetadata;
const REALM_SERVICE = REALM_SERVICE_CONTRACT;

/** Hard Guest-facing cap so a package cannot exhaust Host memory. 7 MiB. */
export const ATTACHMENT_UPLOAD_MAX_BYTES = 7 * 1024 * 1024;

/**
 * Strict RFC 4648 (padded) base64, matching the contract boundary check.
 * Implemented as an O(n) scan: a character-class regex backtracks
 * quadratically on multi-megabyte payloads and overflows the engine on
 * inputs near the 7 MiB cap.
 */
function isValidBase64(text: string): boolean {
  const length = text.length;
  if (length % 4 !== 0) return false;
  const dataLength = length - (text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0);
  for (let index = 0; index < length; index += 1) {
    const character = text[index];
    if (index < dataLength) {
      if (!BASE64_INDEX.has(character)) return false;
    } else if (character !== '=') {
      return false;
    }
  }
  // A final quad holds 0, 2 or 3 data characters ("xx=="/"xxx="); 1 is
  // not representable ("x===" is not RFC 4648).
  return dataLength % 4 !== 1;
}

const BASE64_ALPHABET =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const BASE64_INDEX = new Map(
  Array.from(BASE64_ALPHABET, (character, index) => [character, index])
);

export type AttachmentUploadType =
  | 'image'
  | 'document'
  | 'audio'
  | 'video'
  | 'other';

const ATTACHMENT_UPLOAD_TYPES: readonly AttachmentUploadType[] = [
  'image',
  'document',
  'audio',
  'video',
  'other',
];

/** Decoded byte length of a validated, padded base64 string. */
export function base64DecodedLength(text: string): number {
  const padding = text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0;
  return (text.length / 4) * 3 - padding;
}

/** Decode padded base64 without relying on `atob` (unavailable in Hermes). */
export function decodeBase64(text: string): Uint8Array {
  if (!isValidBase64(text)) {
    throw new Error('base64 must be standard base64 (RFC 4648, padded)');
  }
  const bytes = new Uint8Array(base64DecodedLength(text));
  let offset = 0;
  for (let index = 0; index < text.length; index += 4) {
    const chunk = [
      BASE64_INDEX.get(text[index]) ?? 0,
      BASE64_INDEX.get(text[index + 1]) ?? 0,
      text[index + 2] === '=' ? 0 : (BASE64_INDEX.get(text[index + 2]) ?? 0),
      text[index + 3] === '=' ? 0 : (BASE64_INDEX.get(text[index + 3]) ?? 0),
    ];
    const combined =
      (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6) | chunk[3];
    if (offset < bytes.length) bytes[offset++] = (combined >> 16) & 0xff;
    if (offset < bytes.length) bytes[offset++] = (combined >> 8) & 0xff;
    if (offset < bytes.length) bytes[offset++] = combined & 0xff;
  }
  return bytes;
}

/** Encode bytes as padded base64 without relying on `btoa`. */
export function encodeBase64(bytes: Uint8Array): string {
  let output = '';
  for (let index = 0; index < bytes.length; index += 3) {
    const remaining = bytes.length - index;
    const combined =
      (bytes[index] << 16) |
      (remaining > 1 ? bytes[index + 1] << 8 : 0) |
      (remaining > 2 ? bytes[index + 2] : 0);
    output += BASE64_ALPHABET[(combined >> 18) & 0x3f];
    output += BASE64_ALPHABET[(combined >> 12) & 0x3f];
    output += remaining > 1 ? BASE64_ALPHABET[(combined >> 6) & 0x3f] : '=';
    output += remaining > 2 ? BASE64_ALPHABET[combined & 0x3f] : '=';
  }
  return output;
}

export type AttachmentUploadInput = {
  filename: string;
  /** Strict RFC 4648 padded base64, at most 7 MiB decoded. */
  base64: string;
  /** Defaults to `document`. */
  type?: AttachmentUploadType;
  /** Caller-chosen attachment id; the Host generates one when omitted. */
  id?: string;
};

export type AttachmentUploadOutput = {
  attachmentId: string;
};

/** Terminal result event, normalized from the native broadcast. Internal. */
export type AttachmentUploadResultEvent = {
  /** Matches the `resultStreamId` generated by the Host. */
  resultStreamId: string;
  status: number;
  statusCode: number;
  attachmentId?: string;
  errorMessage?: string;
};

export type AttachmentResultSubscription = {
  remove: () => void;
};

/**
 * The native side of the upload protocol, injected by the Host app and never
 * exposed to the Guest.
 */
export type AttachmentUploadNativeAdapter = {
  prepareAttachmentUpload: (input: {
    streamId: string;
    resultStreamId: string;
    id: string;
    filename: string;
    type: AttachmentUploadType;
    sizeBytes: number;
    checksum: string;
    checksumAlgorithm: (typeof ATTACHMENT_CONTRACT.checksumAlgorithms)['md5'];
  }) => Promise<void>;
  sendAttachmentChunk: (input: {
    streamId: string;
    chunkId: string;
    sequence: number;
    base64: string;
    eos: boolean;
    metadata: Record<string, string>;
  }) => Promise<void>;
  /** Clears the native upload session; called with the chunk streamId. */
  unregisterStream: (streamId: string) => Promise<void>;
  addResultListener: (
    handler: (event: AttachmentUploadResultEvent) => void
  ) => AttachmentResultSubscription;
};

let streamIdSequence = 0;

/** Default id factory: unique per call and distinct across stream/result. */
function defaultCreateStreamId(): string {
  streamIdSequence += 1;
  return `attachment-${streamIdSequence.toString(36)}-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 10)}`;
}

function md5Hex(bytes: Uint8Array): string {
  // Copy into a fresh buffer: SparkMD5 requires a real ArrayBuffer, and the
  // chunking views below may offset into a larger allocation.
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return SparkMD5.ArrayBuffer.hash(copy.buffer);
}

/**
 * Runs the uploads of one Guest session. Concurrent `upload` calls are
 * allowed and isolated by their `resultStreamId`; `dispose` rejects every
 * pending upload and clears its native session (engine teardown / reload).
 */
export class AttachmentUploadManager {
  private readonly adapter: AttachmentUploadNativeAdapter | undefined;
  private readonly resultTimeoutMs: number;
  private readonly createStreamId: () => string;
  private readonly cancellers = new Set<() => void>();
  private disposed = false;

  constructor(options: {
    adapter: AttachmentUploadNativeAdapter | undefined;
    /** Defaults to the generated 30 second attachment result timeout. */
    resultTimeoutMs?: number;
    /** Test hook for deterministic stream ids. */
    createStreamId?: () => string;
  }) {
    this.adapter = options.adapter;
    this.resultTimeoutMs =
      options.resultTimeoutMs ?? ATTACHMENT_CONTRACT.resultTimeoutMs;
    this.createStreamId = options.createStreamId ?? defaultCreateStreamId;
  }

  /** True when the native bridge supplied a usable adapter. */
  get available(): boolean {
    return this.adapter !== undefined;
  }

  async upload(input: AttachmentUploadInput): Promise<AttachmentUploadOutput> {
    if (this.disposed) {
      throw new Error('The attachment uploader is disposed.');
    }
    if (!this.adapter) {
      // Fail closed: no HTTP or legacy fallback exists for this transport.
      throw new Error('The native attachment upload bridge is unavailable.');
    }
    const adapter = this.adapter;
    const { filename, base64, type, id } = parseAttachmentUploadInput(input);
    const bytes = decodeBase64(base64);
    const attachmentId = id ?? this.createStreamId();
    // The Host owns both ids; the native prepare call carries them and never
    // generates a resultStreamId itself.
    const streamId = this.createStreamId();
    const resultStreamId = this.createStreamId();
    if (streamId.length === 0 || resultStreamId.length === 0) {
      throw new Error('Attachment upload stream ids must be non-empty.');
    }
    if (streamId === resultStreamId) {
      throw new Error('Attachment upload stream ids must be distinct.');
    }

    let settled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let subscription: AttachmentResultSubscription | undefined;
    let detached = false;
    let unregisterSent = false;

    // Listener removal and the native session cleanup are idempotent so
    // every exit path (success, FAILED, timeout, prepare/chunk failure,
    // dispose) can call `cleanup` without coordination. Cleanup failures
    // never change the upload outcome.
    const cleanup = () => {
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
      if (!detached) {
        detached = true;
        try {
          subscription?.remove();
        } catch {
          // Listener removal must never change the upload outcome.
        }
      }
      if (!unregisterSent) {
        unregisterSent = true;
        try {
          Promise.resolve(adapter.unregisterStream(streamId)).catch(() => {
            // Native session cleanup is best-effort.
          });
        } catch {
          // A synchronously throwing adapter must not change the outcome.
        }
      }
    };

    let resolveOperation!: (output: AttachmentUploadOutput) => void;
    let rejectOperation!: (cause: unknown) => void;
    const operation = new Promise<AttachmentUploadOutput>((resolve, reject) => {
      resolveOperation = resolve;
      rejectOperation = reject;
    });

    const finish = (
      outcome: { ok: true; attachmentId: string } | { ok: false; cause: unknown }
    ) => {
      if (settled) return;
      settled = true;
      cleanup();
      if (outcome.ok) {
        resolveOperation({ attachmentId: outcome.attachmentId });
      } else {
        rejectOperation(
          outcome.cause instanceof Error
            ? outcome.cause
            : new Error(String(outcome.cause))
        );
      }
    };

    // Let `dispose` reject this upload while it is pending.
    const cancel = () => {
      finish({
        ok: false,
        cause: new Error('The attachment uploader is disposed.'),
      });
    };
    this.cancellers.add(cancel);

    try {
      // 1. Register the result waiter before any native call so an early
      //    broadcast can never be missed. A throwing registration must stop
      //    the upload before Prepare starts.
      try {
        subscription = adapter.addResultListener((event) => {
          if (settled) return;
          // The broadcast is shared; only this upload's terminal event counts.
          if (event.resultStreamId !== resultStreamId) return;
          if (event.status === ATTACHMENT_CONTRACT.uploadStatus.completed) {
            if (event.statusCode === ATTACHMENT_CONTRACT.statusCode.ok) {
              if (
                typeof event.attachmentId === 'string' &&
                event.attachmentId.length > 0
              ) {
                finish({ ok: true, attachmentId: event.attachmentId });
              } else {
                finish({
                  ok: false,
                  cause: new Error(
                    'Attachment upload completed without an attachment id.'
                  ),
                });
              }
            } else {
              finish({
                ok: false,
                cause: new Error(
                  event.errorMessage ??
                    `Attachment upload failed with statusCode ${event.statusCode}.`
                ),
              });
            }
          } else if (event.status === ATTACHMENT_CONTRACT.uploadStatus.failed) {
            finish({
              ok: false,
              cause: new Error(event.errorMessage ?? 'Attachment upload failed.'),
            });
          }
          // Any other status is not terminal; keep waiting until the timeout.
        });
      } catch (cause) {
        finish({ ok: false, cause });
        return await operation;
      }

      timer = setTimeout(() => {
        finish({
          ok: false,
          cause: new Error(
            `Attachment upload result timed out after ${this.resultTimeoutMs} ms.`
          ),
        });
      }, this.resultTimeoutMs);
      // An emitter that delivers synchronously during registration may have
      // already settled before the timer existed.
      if (settled && timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }

      // 2. Prepare (with both ids) and stream the chunks serially. Every
      //    await is followed by a `settled` check so a timeout or dispose
      //    stops the remaining native calls immediately.
      void (async () => {
        try {
          if (settled) return;
          await adapter.prepareAttachmentUpload({
            streamId,
            resultStreamId,
            id: attachmentId,
            filename,
            type,
            sizeBytes: bytes.byteLength,
            checksum: md5Hex(bytes),
            checksumAlgorithm: ATTACHMENT_CONTRACT.checksumAlgorithms.md5,
          });
          if (settled) return;
          await sendChunks(adapter, attachmentId, streamId, bytes, () => !settled);
        } catch (cause) {
          finish({ ok: false, cause });
        }
      })();

      return await operation;
    } finally {
      this.cancellers.delete(cancel);
      // Covers the synchronous-throw path (e.g. a throwing addResultListener);
      // settled uploads already cleaned up inside `finish`.
      cleanup();
    }
  }

  /** Reject pending uploads and clear their native sessions. */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    for (const cancel of this.cancellers) {
      try {
        cancel();
      } catch {
        // A throwing canceller must not stop disposal of the rest.
      }
    }
    this.cancellers.clear();
  }
}

/**
 * Stream the payload in 48 KiB chunks. For a non-empty file the last data
 * chunk itself carries `eos: true` and the EOS metadata; only an empty file
 * sends a single `sequence: 1` empty EOS chunk.
 */
async function sendChunks(
  adapter: AttachmentUploadNativeAdapter,
  attachmentId: string,
  streamId: string,
  bytes: Uint8Array,
  shouldContinue: () => boolean
): Promise<void> {
  const eosMetadata = {
    [STREAM_METADATA.eosKey]: STREAM_METADATA.terminalValue,
  };
  const chunkSize = ATTACHMENT_CONTRACT.chunkSizeBytes;
  if (bytes.byteLength === 0) {
    if (!shouldContinue()) return;
    await adapter.sendAttachmentChunk({
      streamId,
      chunkId: `${attachmentId}-1`,
      sequence: 1,
      base64: '',
      eos: true,
      metadata: eosMetadata,
    });
    return;
  }
  const chunkCount = Math.ceil(bytes.byteLength / chunkSize);
  for (let index = 0; index < chunkCount; index += 1) {
    if (!shouldContinue()) return;
    const sequence = index + 1;
    const last = sequence === chunkCount;
    await adapter.sendAttachmentChunk({
      streamId,
      chunkId: `${attachmentId}-${sequence}`,
      sequence,
      base64: encodeBase64(
        bytes.subarray(
          index * chunkSize,
          Math.min((index + 1) * chunkSize, bytes.byteLength)
        )
      ),
      eos: last,
      metadata: last ? eosMetadata : {},
    });
  }
}

/** Boundary validation shared by the contract schema and the manager. */
export function parseAttachmentUploadInput(
  value: unknown
): {
  filename: string;
  base64: string;
  type: AttachmentUploadType;
  id?: string;
} {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('uploadAttachment input must be an object');
  }
  const input = value as Record<string, unknown>;
  if (typeof input.filename !== 'string' || input.filename.length === 0) {
    throw new Error('uploadAttachment input.filename must be a non-empty string');
  }
  if (typeof input.base64 !== 'string' || !isValidBase64(input.base64)) {
    throw new Error(
      'uploadAttachment input.base64 must be standard base64 (RFC 4648, padded)'
    );
  }
  if (base64DecodedLength(input.base64) > ATTACHMENT_UPLOAD_MAX_BYTES) {
    throw new Error(
      `uploadAttachment input.base64 must decode to at most ${ATTACHMENT_UPLOAD_MAX_BYTES} bytes`
    );
  }
  const type =
    input.type === undefined ? 'document' : parseAttachmentUploadType(input.type);
  let id: string | undefined;
  if (input.id !== undefined) {
    if (typeof input.id !== 'string' || input.id.length === 0) {
      throw new Error(
        'uploadAttachment input.id must be a non-empty string when present'
      );
    }
    id = input.id;
  }
  return { filename: input.filename, base64: input.base64, type, id };
}

function parseAttachmentUploadType(value: unknown): AttachmentUploadType {
  if (
    typeof value !== 'string' ||
    !(ATTACHMENT_UPLOAD_TYPES as readonly string[]).includes(value)
  ) {
    throw new Error(
      `uploadAttachment input.type must be one of ${ATTACHMENT_UPLOAD_TYPES.join(', ')}`
    );
  }
  return value as AttachmentUploadType;
}

/** Minimal React Native emitter surface used for the result broadcast. */
export type AttachmentResultEmitter = {
  addListener: (
    eventType: string,
    listener: (event: unknown) => void
  ) => { remove: () => void };
};

/** Loosely typed per-tab `RealmService` native module methods. */
export type RealmServiceAttachmentBridge = {
  prepareAttachmentUpload?: (
    tabId: string,
    input: Record<string, unknown>
  ) => Promise<unknown>;
  sendAttachmentChunk?: (
    tabId: string,
    input: Record<string, unknown>
  ) => Promise<unknown>;
  unregisterStream?: (tabId: string, streamId: string) => Promise<unknown>;
};

/**
 * Normalize the native `RealmServiceAttachmentResult` event. The Swift/IPC
 * layer settles on `streamId`, so it wins; `resultStreamId` stays accepted
 * for cross-layer alignment. `tabId` must match this tab: one renderer
 * process may own several tabs and the event emitter is process-wide.
 */
function mapNativeResultEvent(
  raw: unknown,
  tabId: string
): AttachmentUploadResultEvent | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const event = raw as Record<string, unknown>;
  if (event[BRIDGE_KEYS.tabId] !== tabId) return undefined;
  const streamId = event[BRIDGE_KEYS.streamId];
  const resultStreamId = event[BRIDGE_KEYS.resultStreamId];
  const status = event[BRIDGE_KEYS.status];
  if (
    (typeof streamId !== 'string' || streamId.length === 0) &&
    (typeof resultStreamId !== 'string' || resultStreamId.length === 0)
  ) {
    return undefined;
  }
  if (typeof status !== 'number') return undefined;
  const statusCode = event[BRIDGE_KEYS.statusCode];
  const mapped: AttachmentUploadResultEvent = {
    resultStreamId:
      typeof streamId === 'string' && streamId.length > 0
        ? streamId
        : (resultStreamId as string),
    status,
    statusCode: typeof statusCode === 'number' ? statusCode : -1,
  };
  const attachmentId = event[BRIDGE_KEYS.attachmentId];
  if (typeof attachmentId === 'string') mapped.attachmentId = attachmentId;
  const errorMessage = event[BRIDGE_KEYS.errorMessage];
  if (typeof errorMessage === 'string') mapped.errorMessage = errorMessage;
  return mapped;
}

/**
 * Bind the per-tab `RealmService` native module to the adapter protocol.
 * Returns `undefined` when any required native method or the emitter is
 * absent, so the Host fails closed instead of half-wiring the bridge.
 */
export function createRealmServiceAttachmentAdapter(options: {
  bridge: RealmServiceAttachmentBridge | undefined;
  emitter: AttachmentResultEmitter | undefined;
  /** The owning tab; every native call is scoped to it. */
  tabId: string;
}): AttachmentUploadNativeAdapter | undefined {
  const { bridge, emitter, tabId } = options;
  if (
    !bridge ||
    !tabId ||
    typeof bridge.prepareAttachmentUpload !== 'function' ||
    typeof bridge.sendAttachmentChunk !== 'function' ||
    typeof bridge.unregisterStream !== 'function' ||
    !emitter ||
    typeof emitter.addListener !== 'function'
  ) {
    return undefined;
  }
  const nativeMethods = bridge;
  const resultEventName = REALM_SERVICE.events.attachmentResult;
  return {
    prepareAttachmentUpload: async (input) => {
      await nativeMethods.prepareAttachmentUpload!(tabId, {
        [BRIDGE_KEYS.streamId]: input.streamId,
        [BRIDGE_KEYS.resultStreamId]: input.resultStreamId,
        [BRIDGE_KEYS.id]: input.id,
        [BRIDGE_KEYS.filename]: input.filename,
        [BRIDGE_KEYS.type]: ATTACHMENT_CONTRACT.types[input.type],
        [BRIDGE_KEYS.sizeBytes]: input.sizeBytes,
        [BRIDGE_KEYS.checksum]: input.checksum,
        [BRIDGE_KEYS.checksumAlgorithm]: input.checksumAlgorithm,
      });
    },
    sendAttachmentChunk: async (input) => {
      await nativeMethods.sendAttachmentChunk!(tabId, {
        [BRIDGE_KEYS.streamId]: input.streamId,
        [BRIDGE_KEYS.chunkId]: input.chunkId,
        [BRIDGE_KEYS.sequence]: input.sequence,
        [BRIDGE_KEYS.fileData]: input.base64,
        ...(input.eos ? input.metadata : {}),
      });
    },
    unregisterStream: async (streamId) => {
      await nativeMethods.unregisterStream!(tabId, streamId);
    },
    addResultListener: (handler) => {
      const listener = (raw: unknown) => {
        const mapped = mapNativeResultEvent(raw, tabId);
        if (mapped) handler(mapped);
      };
      return emitter.addListener(resultEventName, listener);
    },
  };
}
