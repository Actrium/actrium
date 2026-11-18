import { describe, expect, test } from 'bun:test';
import SparkMD5 from 'spark-md5';

import { ACTR_BRIDGE_CONTRACT } from './coreContract';
import {
  ATTACHMENT_UPLOAD_MAX_BYTES,
  AttachmentUploadManager,
  base64DecodedLength,
  createRealmServiceAttachmentAdapter,
  decodeBase64,
  encodeBase64,
  type AttachmentUploadNativeAdapter,
  type AttachmentUploadResultEvent,
} from './attachment-upload';

const CHUNK_SIZE = ACTR_BRIDGE_CONTRACT.attachment.chunkSizeBytes; // 49152
const COMPLETED = ACTR_BRIDGE_CONTRACT.attachment.uploadStatus.completed;
const FAILED = ACTR_BRIDGE_CONTRACT.attachment.uploadStatus.failed;
const OK = ACTR_BRIDGE_CONTRACT.attachment.statusCode.ok;

type PrepareRequest = Parameters<
  AttachmentUploadNativeAdapter['prepareAttachmentUpload']
>[0];
type ChunkRequest = Parameters<
  AttachmentUploadNativeAdapter['sendAttachmentChunk']
>[0];

type RecordedCall =
  | { kind: 'prepare'; input: PrepareRequest }
  | { kind: 'chunk'; input: ChunkRequest }
  | { kind: 'unregister'; streamId: string };

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Deterministic in-memory adapter with an externally driven result feed. */
function createRecordingAdapter(options?: {
  unregisterThrows?: boolean;
  removeThrows?: boolean;
}) {
  const calls: RecordedCall[] = [];
  const handlers = new Set<(event: AttachmentUploadResultEvent) => void>();
  let unregisterCount = 0;
  let removeCount = 0;
  const adapter: AttachmentUploadNativeAdapter = {
    prepareAttachmentUpload: async (input) => {
      calls.push({ kind: 'prepare', input });
    },
    sendAttachmentChunk: async (input) => {
      calls.push({ kind: 'chunk', input });
    },
    unregisterStream: async (streamId) => {
      unregisterCount += 1;
      calls.push({ kind: 'unregister', streamId });
      if (options?.unregisterThrows) throw new Error('native unregister exploded');
    },
    addResultListener: (handler) => {
      handlers.add(handler);
      return {
        remove: () => {
          removeCount += 1;
          handlers.delete(handler);
          if (options?.removeThrows) throw new Error('native remove exploded');
        },
      };
    },
  };
  return {
    adapter,
    calls,
    emit: (event: AttachmentUploadResultEvent) => {
      for (const handler of [...handlers]) handler(event);
    },
    handlerCount: () => handlers.size,
    unregisterCount: () => unregisterCount,
    removeCount: () => removeCount,
  };
}

/** Sequential deterministic ids so tests can predict stream/result ids. */
function sequentialIds(prefix: string) {
  let next = 0;
  return () => `${prefix}-${(next += 1)}`;
}

function completedEvent(
  resultStreamId: string,
  overrides?: Partial<AttachmentUploadResultEvent>
): AttachmentUploadResultEvent {
  return {
    resultStreamId,
    status: COMPLETED,
    statusCode: OK,
    attachmentId: 'attachment-1',
    ...overrides,
  };
}

function bytesOf(size: number, seed = 0): Uint8Array {
  const bytes = new Uint8Array(size);
  for (let index = 0; index < size; index += 1) {
    bytes[index] = (index + seed) % 256;
  }
  return bytes;
}

describe('base64 helpers', () => {
  test('round-trips padded base64', () => {
    for (const size of [0, 1, 2, 3, 100, CHUNK_SIZE, CHUNK_SIZE + 1]) {
      const bytes = bytesOf(size);
      expect(decodeBase64(encodeBase64(bytes))).toEqual(bytes);
    }
  });

  test('computes decoded length including padding', () => {
    expect(base64DecodedLength('')).toBe(0);
    expect(base64DecodedLength('AAEC')).toBe(3);
    expect(base64DecodedLength('AQI=')).toBe(2);
    expect(base64DecodedLength('AQ==')).toBe(1);
  });
});

describe('md5 checksums', () => {
  const MD5_VECTORS: ReadonlyArray<[string, string]> = [
    ['', 'd41d8cd98f00b204e9800998ecf8427e'],
    ['abc', '900150983cd24fb0d6963f7d28e17f72'],
    [
      'The quick brown fox jumps over the lazy dog',
      '9e107d9d372bb6826bd81d3542a419d6',
    ],
  ];

  for (const [text, expected] of MD5_VECTORS) {
    test(`prepare carries the md5 of ${JSON.stringify(text.slice(0, 24))}`, async () => {
      const recorder = createRecordingAdapter();
      const manager = new AttachmentUploadManager({
        adapter: recorder.adapter,
        createStreamId: sequentialIds('s'),
      });
      const bytes = new TextEncoder().encode(text);
      const done = manager.upload({
        filename: 'vector.bin',
        id: 'vector-id',
        base64: encodeBase64(bytes),
      });
      await flush();
      const prepare = recorder.calls.find((call) => call.kind === 'prepare');
      expect(prepare).toMatchObject({
        input: {
          sizeBytes: bytes.byteLength,
          checksum: expected,
          checksumAlgorithm: 'md5',
        },
      });
      recorder.emit(completedEvent('s-2'));
      await expect(done).resolves.toEqual({ attachmentId: 'attachment-1' });
    });
  }

  test('prepare carries the md5 of a 48 KiB + 1 payload', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const payload = bytesOf(CHUNK_SIZE + 1, 3);
    const done = manager.upload({
      filename: 'blob.bin',
      base64: encodeBase64(payload),
      id: 'blob-id',
    });
    await flush();
    const prepare = recorder.calls.find((call) => call.kind === 'prepare');
    expect(prepare).toMatchObject({
      input: { checksum: SparkMD5.ArrayBuffer.hash(new Uint8Array(payload).buffer) },
    });
    recorder.emit(completedEvent('s-2'));
    await done;
  });
});

describe('AttachmentUploadManager input validation', () => {
  test('fails closed when the native bridge is missing', async () => {
    const manager = new AttachmentUploadManager({ adapter: undefined });
    expect(manager.available).toBe(false);
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' })
    ).rejects.toThrow('native attachment upload bridge is unavailable');
  });

  test('rejects malformed base64 and oversized payloads before prepare', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({ adapter: recorder.adapter });
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'not base64!!' })
    ).rejects.toThrow('standard base64');
    await expect(manager.upload({ filename: 'a.txt', base64: 'AAE' })).rejects.toThrow(
      'standard base64'
    );
    const oversized = encodeBase64(bytesOf(ATTACHMENT_UPLOAD_MAX_BYTES + 1));
    await expect(
      manager.upload({ filename: 'a.txt', base64: oversized })
    ).rejects.toThrow('at most');
    expect(recorder.calls).toEqual([]);
  });

  test('rejects a missing filename, an unknown type and an empty id', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({ adapter: recorder.adapter });
    await expect(manager.upload({ filename: '', base64: 'AAEC' })).rejects.toThrow(
      'filename'
    );
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', type: 'archive' as never })
    ).rejects.toThrow('type');
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: '' })
    ).rejects.toThrow('id');
    expect(recorder.calls).toEqual([]);
  });

  test('accepts exactly 7 MiB', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      resultTimeoutMs: 100,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({
      filename: 'big.bin',
      base64: encodeBase64(bytesOf(ATTACHMENT_UPLOAD_MAX_BYTES)),
      id: 'big-id',
    });
    await flush();
    recorder.emit(completedEvent('s-2', { attachmentId: 'attachment-7m' }));
    await expect(done).resolves.toEqual({ attachmentId: 'attachment-7m' });
  });
});

describe('AttachmentUploadManager protocol', () => {
  test('generates distinct ids, registers the listener before prepare and sends both ids', async () => {
    const recorder = createRecordingAdapter();
    let sawListenerBeforePrepare = false;
    const ordered: string[] = [];
    const observing: AttachmentUploadNativeAdapter = {
      ...recorder.adapter,
      addResultListener: (handler) => {
        ordered.push('listener');
        return recorder.adapter.addResultListener(handler);
      },
      prepareAttachmentUpload: async (input) => {
        ordered.push('prepare');
        sawListenerBeforePrepare = recorder.handlerCount() === 1;
        return recorder.adapter.prepareAttachmentUpload(input);
      },
    };
    const manager = new AttachmentUploadManager({
      adapter: observing,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({
      filename: 'report.pdf',
      base64: encodeBase64(bytesOf(64, 7)),
      id: 'fixed-id',
    });
    await flush();
    expect(ordered).toEqual(['listener', 'prepare']);
    expect(sawListenerBeforePrepare).toBe(true);
    const prepare = recorder.calls.find((call) => call.kind === 'prepare');
    expect(prepare).toMatchObject({
      input: {
        streamId: 's-1',
        resultStreamId: 's-2',
        id: 'fixed-id',
        filename: 'report.pdf',
        type: 'document',
        sizeBytes: 64,
        checksumAlgorithm: 'md5',
      },
    });
    expect((prepare as { input: PrepareRequest }).input.streamId).not.toBe(
      (prepare as { input: PrepareRequest }).input.resultStreamId
    );
    recorder.emit(completedEvent('s-2', { attachmentId: 'fixed-id' }));
    await expect(done).resolves.toEqual({ attachmentId: 'fixed-id' });
  });

  test('streams 48 KiB chunks serially; the last data chunk is its own EOS', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const payload = bytesOf(CHUNK_SIZE * 2 + 10);
    const done = manager.upload({
      filename: 'blob.bin',
      base64: encodeBase64(payload),
      type: 'image',
      id: 'blob-id',
    });
    await flush();
    const chunks = recorder.calls.filter((call) => call.kind === 'chunk');
    expect(chunks).toHaveLength(3);
    const first = (chunks[0] as { input: ChunkRequest }).input;
    const second = (chunks[1] as { input: ChunkRequest }).input;
    const last = (chunks[2] as { input: ChunkRequest }).input;
    expect(decodeBase64(first.base64).byteLength).toBe(CHUNK_SIZE);
    expect(decodeBase64(second.base64).byteLength).toBe(CHUNK_SIZE);
    expect(decodeBase64(last.base64).byteLength).toBe(10);
    expect(first).toMatchObject({
      streamId: 's-1',
      sequence: 1,
      eos: false,
      metadata: {},
    });
    expect(second).toMatchObject({ sequence: 2, eos: false, metadata: {} });
    expect(last).toMatchObject({
      streamId: 's-1',
      sequence: 3,
      eos: true,
      metadata: {
        [ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey]:
          ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
      },
    });
    expect(decodeBase64(first.base64)).toEqual(payload.subarray(0, CHUNK_SIZE));
    expect(decodeBase64(second.base64)).toEqual(
      payload.subarray(CHUNK_SIZE, CHUNK_SIZE * 2)
    );
    expect(decodeBase64(last.base64)).toEqual(payload.subarray(CHUNK_SIZE * 2));

    recorder.emit(completedEvent('s-2'));
    await expect(done).resolves.toEqual({ attachmentId: 'attachment-1' });
  });

  test('48 KiB exactly produces a single self-terminating data chunk', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({
      filename: 'exact.bin',
      base64: encodeBase64(bytesOf(CHUNK_SIZE)),
      id: 'exact-id',
    });
    await flush();
    const chunks = recorder.calls.filter((call) => call.kind === 'chunk');
    expect(chunks).toHaveLength(1);
    expect((chunks[0] as { input: ChunkRequest }).input).toMatchObject({
      sequence: 1,
      eos: true,
      metadata: {
        [ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey]:
          ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
      },
    });
    recorder.emit(completedEvent('s-2'));
    await done;
  });

  test('a zero-byte file sends a single empty EOS chunk with sequence 1', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'empty.txt', base64: '', id: 'empty-id' });
    await flush();
    const chunks = recorder.calls.filter((call) => call.kind === 'chunk');
    expect(chunks).toHaveLength(1);
    expect((chunks[0] as { input: ChunkRequest }).input).toEqual({
      streamId: 's-1',
      chunkId: expect.stringContaining('-1'),
      sequence: 1,
      base64: '',
      eos: true,
      metadata: {
        [ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey]:
          ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
      },
    });
    recorder.emit(completedEvent('s-2'));
    await expect(done).resolves.toEqual({ attachmentId: 'attachment-1' });
  });

  test('chunks are serialized: the next chunk waits for the previous one', async () => {
    const order: number[] = [];
    let gateUsed = false;
    let releaseChunk!: () => void;
    const gate = new Promise<void>((resolve) => {
      releaseChunk = resolve;
    });
    const recorder = createRecordingAdapter();
    const gatedAdapter: AttachmentUploadNativeAdapter = {
      ...recorder.adapter,
      sendAttachmentChunk: async (input) => {
        if (!gateUsed) {
          gateUsed = true;
          await gate;
        }
        order.push(input.sequence);
      },
    };
    const manager = new AttachmentUploadManager({
      adapter: gatedAdapter,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({
      filename: 'blob.bin',
      base64: encodeBase64(bytesOf(CHUNK_SIZE * 2)),
      id: 'gate-id',
    });
    await flush();
    expect(order).toEqual([]); // first chunk still in flight
    releaseChunk();
    await flush();
    expect(order).toEqual([1, 2]);
    recorder.emit(completedEvent('s-2'));
    await done;
  });

  test('ignores results for other streams and non-terminal statuses', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    recorder.emit(completedEvent('some-other-stream'));
    recorder.emit(
      completedEvent('s-2', {
        status: ACTR_BRIDGE_CONTRACT.attachment.uploadStatus.unknown,
      })
    );
    recorder.emit(completedEvent('s-2', { attachmentId: 'attachment-9' }));
    await expect(done).resolves.toEqual({ attachmentId: 'attachment-9' });
  });

  test('FAILED rejects immediately without waiting for the timeout', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      resultTimeoutMs: 60_000,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    recorder.emit(
      completedEvent('s-2', { status: FAILED, errorMessage: 'virus detected' })
    );
    await expect(done).rejects.toThrow('virus detected');
  });

  test('COMPLETED with a non-zero statusCode rejects immediately', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      resultTimeoutMs: 60_000,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    recorder.emit(
      completedEvent('s-2', { statusCode: 7, errorMessage: 'quota exceeded' })
    );
    await expect(done).rejects.toThrow('quota exceeded');
  });

  test('an illegal success (no attachment id) fails explicitly', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      resultTimeoutMs: 60_000,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    recorder.emit(completedEvent('s-2', { attachmentId: undefined }));
    await expect(done).rejects.toThrow('without an attachment id');
  });

  test('times out when no terminal result arrives', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      resultTimeoutMs: 20,
      createStreamId: sequentialIds('s'),
    });
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' })
    ).rejects.toThrow('timed out after 20 ms');
    await flush();
    expect(recorder.handlerCount()).toBe(0);
  });

  test('prepare failure removes the listener and unregisters the stream', async () => {
    const recorder = createRecordingAdapter();
    const failing: AttachmentUploadNativeAdapter = {
      ...recorder.adapter,
      prepareAttachmentUpload: async () => {
        throw new Error('prepare rejected');
      },
    };
    const manager = new AttachmentUploadManager({
      adapter: failing,
      resultTimeoutMs: 60_000,
      createStreamId: sequentialIds('s'),
    });
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' })
    ).rejects.toThrow('prepare rejected');
    await flush();
    expect(recorder.handlerCount()).toBe(0);
    expect(recorder.removeCount()).toBe(1);
    expect(recorder.unregisterCount()).toBe(1);
    expect(recorder.calls).toEqual([{ kind: 'unregister', streamId: 's-1' }]);
  });

  test('chunk failure removes the listener and unregisters the stream', async () => {
    const recorder = createRecordingAdapter();
    const failing: AttachmentUploadNativeAdapter = {
      ...recorder.adapter,
      sendAttachmentChunk: async () => {
        throw new Error('chunk rejected');
      },
    };
    const manager = new AttachmentUploadManager({
      adapter: failing,
      resultTimeoutMs: 60_000,
      createStreamId: sequentialIds('s'),
    });
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' })
    ).rejects.toThrow('chunk rejected');
    await flush();
    expect(recorder.handlerCount()).toBe(0);
    expect(recorder.removeCount()).toBe(1);
    expect(recorder.unregisterCount()).toBe(1);
  });

  test('native cleanup failures never change the upload outcome', async () => {
    const recorder = createRecordingAdapter({
      unregisterThrows: true,
      removeThrows: true,
    });
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    recorder.emit(completedEvent('s-2'));
    await expect(done).resolves.toEqual({ attachmentId: 'attachment-1' });
    await flush();
    expect(recorder.unregisterCount()).toBe(1);
    expect(recorder.removeCount()).toBe(1);
  });

  test('unregisterStream is called exactly once on the success path', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const done = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    recorder.emit(completedEvent('s-2'));
    await done;
    await flush();
    expect(recorder.unregisterCount()).toBe(1);
    expect(recorder.removeCount()).toBe(1);
    expect(recorder.handlerCount()).toBe(0);
  });

  test('runs parallel uploads isolated by resultStreamId', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const first = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'one' });
    const second = manager.upload({ filename: 'b.txt', base64: 'AQID', id: 'two' });
    await flush();
    // Distinct ids per upload: 'one' used s-1/s-2, 'two' used s-3/s-4.
    recorder.emit(completedEvent('s-4', { attachmentId: 'two' }));
    recorder.emit(completedEvent('s-2', { attachmentId: 'one' }));
    await expect(first).resolves.toEqual({ attachmentId: 'one' });
    await expect(second).resolves.toEqual({ attachmentId: 'two' });
    await flush();
    expect(recorder.handlerCount()).toBe(0);
    expect(recorder.unregisterCount()).toBe(2);
  });

  test('dispose rejects pending uploads and unregisters their streams', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const pending = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    manager.dispose();
    await expect(pending).rejects.toThrow('disposed');
    await flush();
    expect(recorder.handlerCount()).toBe(0);
    expect(recorder.unregisterCount()).toBe(1);
    // Uploads are refused after disposal.
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' })
    ).rejects.toThrow('disposed');
  });

  test('dispose is idempotent and never double-unregisters', async () => {
    const recorder = createRecordingAdapter();
    const manager = new AttachmentUploadManager({
      adapter: recorder.adapter,
      createStreamId: sequentialIds('s'),
    });
    const pending = manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' });
    await flush();
    manager.dispose();
    manager.dispose();
    await expect(pending).rejects.toThrow('disposed');
    await flush();
    expect(recorder.unregisterCount()).toBe(1);
  });

  test('dispose during an in-flight chunk stops every later chunk', async () => {
    const recorder = createRecordingAdapter();
    let releaseFirst!: () => void;
    const firstChunk = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const gated: AttachmentUploadNativeAdapter = {
      ...recorder.adapter,
      sendAttachmentChunk: async (input) => {
        if (input.sequence === 1) await firstChunk;
        return recorder.adapter.sendAttachmentChunk(input);
      },
    };
    const manager = new AttachmentUploadManager({
      adapter: gated,
      resultTimeoutMs: 60_000,
      createStreamId: sequentialIds('s'),
    });
    const pending = manager.upload({
      filename: 'blob.bin',
      base64: encodeBase64(bytesOf(CHUNK_SIZE * 3)),
      id: 'stop-id',
    });
    await flush();
    manager.dispose();
    await expect(pending).rejects.toThrow('disposed');
    releaseFirst();
    await flush();
    expect(recorder.calls.filter((call) => call.kind === 'chunk')).toHaveLength(1);
    expect(recorder.unregisterCount()).toBe(1);
  });

  test('a throwing result listener fails before Prepare starts', async () => {
    const recorder = createRecordingAdapter();
    const throwing: AttachmentUploadNativeAdapter = {
      ...recorder.adapter,
      addResultListener: () => {
        throw new Error('listener registration exploded');
      },
    };
    const manager = new AttachmentUploadManager({
      adapter: throwing,
      createStreamId: sequentialIds('s'),
    });
    await expect(
      manager.upload({ filename: 'a.txt', base64: 'AAEC', id: 'a-id' })
    ).rejects.toThrow('listener registration exploded');
    await flush();
    expect(
      recorder.calls.filter((call) => call.kind !== 'unregister')
    ).toEqual([]);
  });
});

describe('createRealmServiceAttachmentAdapter', () => {
  const keys = ACTR_BRIDGE_CONTRACT.keys;
  const tabId = 'tab-1';

  function createBridge() {
    const sent: unknown[] = [];
    const listeners = new Map<string, Array<(event: unknown) => void>>();
    const bridge = {
      sent,
      prepareAttachmentUpload: async (
        callTabId: string,
        input: Record<string, unknown>
      ) => {
        sent.push(['prepare', callTabId, input]);
      },
      sendAttachmentChunk: async (
        callTabId: string,
        input: Record<string, unknown>
      ) => {
        sent.push(['chunk', callTabId, input]);
      },
      unregisterStream: async (callTabId: string, streamId: string) => {
        sent.push(['unregister', callTabId, streamId]);
      },
    };
    const emitter = {
      addListener: (eventType: string, listener: (event: unknown) => void) => {
        const list = listeners.get(eventType) ?? [];
        list.push(listener);
        listeners.set(eventType, list);
        return {
          remove: () => {
            const current = listeners.get(eventType) ?? [];
            listeners.set(
              eventType,
              current.filter((candidate) => candidate !== listener)
            );
          },
        };
      },
    };
    return { bridge, emitter, listeners };
  }

  test('returns undefined unless every native method and the emitter are present', () => {
    const { bridge, emitter } = createBridge();
    expect(
      createRealmServiceAttachmentAdapter({ bridge: undefined, emitter, tabId })
    ).toBeUndefined();
    expect(
      createRealmServiceAttachmentAdapter({ bridge, emitter: undefined, tabId })
    ).toBeUndefined();
    expect(
      createRealmServiceAttachmentAdapter({ bridge, emitter, tabId: '' })
    ).toBeUndefined();
    const { prepareAttachmentUpload, ...rest } = bridge;
    expect(
      createRealmServiceAttachmentAdapter({ bridge: rest, emitter, tabId })
    ).toBeUndefined();
  });

  test('maps prepare and chunk through the generated keys', async () => {
    const { bridge, emitter, listeners } = createBridge();
    const adapter = createRealmServiceAttachmentAdapter({ bridge, emitter, tabId });
    expect(adapter).toBeDefined();

    await adapter!.prepareAttachmentUpload({
      streamId: 'stream-1',
      resultStreamId: 'result-1',
      id: 'id-1',
      filename: 'photo.png',
      type: 'image',
      sizeBytes: 3,
      checksum: 'abc',
      checksumAlgorithm: 'md5',
    });
    expect(bridge.sent[0]).toEqual([
      'prepare',
      tabId,
      {
        [keys.streamId]: 'stream-1',
        [keys.resultStreamId]: 'result-1',
        [keys.id]: 'id-1',
        [keys.filename]: 'photo.png',
        [keys.type]: ACTR_BRIDGE_CONTRACT.attachment.types.image,
        [keys.sizeBytes]: 3,
        [keys.checksum]: 'abc',
        [keys.checksumAlgorithm]: 'md5',
      },
    ]);

    await adapter!.sendAttachmentChunk({
      streamId: 'stream-1',
      chunkId: 'id-1-1',
      sequence: 1,
      base64: 'AAEC',
      eos: true,
      metadata: {
        [ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey]:
          ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
      },
    });
    expect(bridge.sent[1]).toEqual([
      'chunk',
      tabId,
      {
        [keys.streamId]: 'stream-1',
        [keys.chunkId]: 'id-1-1',
        [keys.sequence]: 1,
        [keys.fileData]: 'AAEC',
        [ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey]:
          ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
      },
    ]);

    await adapter!.unregisterStream('stream-1');
    expect(bridge.sent[2]).toEqual(['unregister', tabId, 'stream-1']);
    expect(listeners.size).toBe(0);
  });

  test('maps the RealmServiceAttachmentResult event, preferring streamId', async () => {
    const { bridge, emitter, listeners } = createBridge();
    const adapter = createRealmServiceAttachmentAdapter({ bridge, emitter, tabId });
    let mapped: AttachmentUploadResultEvent | undefined;
    const subscription = adapter!.addResultListener((event) => {
      mapped = event;
    });
    const eventName = 'RealmServiceAttachmentResult';
    expect(listeners.get(eventName)).toHaveLength(1);

    // Swift/IPC settles on streamId; it must win over resultStreamId.
    listeners.get(eventName)![0]({
      [keys.tabId]: tabId,
      [keys.streamId]: 'stream-1',
      [keys.resultStreamId]: 'ignored',
      [keys.status]: COMPLETED,
      [keys.statusCode]: OK,
      [keys.attachmentId]: 'id-1',
    });
    expect(mapped).toEqual({
      resultStreamId: 'stream-1',
      status: COMPLETED,
      statusCode: OK,
      attachmentId: 'id-1',
    });

    // resultStreamId alone still aligns across layers.
    listeners.get(eventName)![0]({
      [keys.tabId]: tabId,
      [keys.resultStreamId]: 'result-2',
      [keys.status]: FAILED,
      [keys.statusCode]: 3,
      [keys.errorMessage]: 'boom',
    });
    expect(mapped).toEqual({
      resultStreamId: 'result-2',
      status: FAILED,
      statusCode: 3,
      errorMessage: 'boom',
    });

    // Results emitted for another tab never reach this uploader.
    mapped = undefined;
    listeners.get(eventName)![0]({
      [keys.tabId]: 'tab-2',
      [keys.streamId]: 'stream-1',
      [keys.status]: COMPLETED,
      [keys.statusCode]: OK,
      [keys.attachmentId]: 'foreign',
    });
    expect(mapped).toBeUndefined();

    // Events without a matching tab and stream are dropped.
    listeners.get(eventName)![0]({ [keys.status]: COMPLETED });
    expect(mapped).toBeUndefined();

    subscription.remove();
    expect(listeners.get(eventName)).toHaveLength(0);
  });
});
