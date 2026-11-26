import { defineKeelContract, rpc, subscription } from 'keel/contract';
import { isAllowedThumbnailBase64 } from './image-asset';

import {
  ATTACHMENT_UPLOAD_MAX_BYTES,
  type AttachmentUploadType,
  parseAttachmentUploadInput,
  parseAttachmentUploadType,
} from './attachment-upload';
import type { TTSSpeakInput, TTSSpeakOutput, TTSVoice } from './guest-tts';

/**
 * Host capabilities declared to Guests (v1 runtime spec, section 5.2/5.3).
 *
 * The contract defines Host-side boundary schemas. The effective capability
 * set only includes supported implementations (see `capability-model.ts`).
 *
 * The host fixes the Service target to the current Service ActorId; the Guest
 * never supplies an ActorId. Business routes stay opaque strings here and are
 * checked against the package capability manifest once route gating lands.
 */

export type ServiceCallInput = {
  route: string;
  payloadBase64: string;
  timeoutMs?: number;
};

export type ServiceCallOutput = {
  payloadBase64: string;
};

export type ServiceTellInput = {
  route: string;
  payloadBase64: string;
};

export type ServiceEvent = {
  event: string;
  payloadBase64: string;
};

export type ServiceStreamInput = { streamId: string };
export type ServiceStreamChunk = ServiceStreamInput & { sequence: string; payloadBase64: string };
export type ServiceStreamSendInput = ServiceStreamChunk & { route: string };
const maxServiceStreamPayload = 7 * 1024 * 1024;

function parseServiceStreamInput(value: unknown): ServiceStreamInput {
  const input = asObject(value, 'Service DataStream');
  const streamId = requireString(input.streamId, 'Service DataStream ID');
  if (!/^[A-Za-z0-9_-]{1,128}$/.test(streamId)) throw new Error('Invalid Service DataStream ID.');
  return { streamId };
}

/**
 * Fixed transport upload exposed on `host:service`. Unlike `call`/`tell`/
 * `on`, the Guest cannot name a route here: the Host owns the whole
 * prepare/chunk/result protocol natively.
 */
export type ServiceUploadAttachmentInput = {
  fileId?: undefined;
  filename: string;
  base64: string;
  /** Defaults to `document`. */
  type?: AttachmentUploadType;
  /** Optional caller-chosen attachment id. */
  id?: string;
} | {
  fileId: string;
  type?: AttachmentUploadType;
  id?: string;
};

export type ServiceUploadAttachmentOutput = {
  attachmentId: string;
};

export type AssetsReadInput = { path: string };
export type AssetsReadOutput = { base64: string };

export type ClipboardWriteTextInput = { text: string };
export type TabSetTitleInput = { title: string };

/** Maximum UTF-8 byte size accepted for a guest-set tab title. */
export const TAB_TITLE_MAX_BYTES = 256;
export type LocalStorageKeyInput = { key: string };
export type LocalStorageSetInput = LocalStorageKeyInput & { value: string };

export type NotificationsShowInput = { title: string; body?: string };

export type CameraCapturePhotoOutput = { base64: string };

export type MicrophoneRecordInput = { durationMs: number };
export type MicrophoneRecordOutput = { base64: string };

/** Omit continuous to finish after a single utterance. */
export type SpeechRecognizeInput = { locale?: string; continuous?: boolean };
/** Final transcript returned after the native session ends. */
export type SpeechRecognizeOutput = { text: string };
export type SpeechInterimHandler = (result: SpeechRecognizeOutput) => void;
export type SpeechRecordingState = { state: 'recording' | 'finalizing' };
export type SpeechStateHandler = (event: SpeechRecordingState) => void;

export type LocationPositionOutput = {
  latitude: number;
  longitude: number;
  accuracy?: number;
};

export type ExternalLinkOpenInput = { url: string };

export type NavigateInput = { path: string; mode?: 'push' | 'replace' };

export type FileImportFile = { name: string; base64: string; thumbnailBase64?: string };
export type CachedFileImport = {
  fileId: string; name: string; size: number; thumbnailBase64?: string; canPreview?: boolean;
};
export type CachedFileInput = { fileId: string };
export type FileImportPickInput = {
  source?: 'files' | 'photos';
  accept?: string[];
  multiple?: boolean;
  cache?: boolean;
};
export type FileImportPickOutput = { files: (FileImportFile | CachedFileImport)[] };

export type FileExportSaveInput = { name: string; base64: string };
export type FileExportSaveOutput = { saved: boolean };

export type ShareInput = { text?: string; url?: string };

export type NetRequestMethod = 'GET' | 'POST';
export type NetRequestInput = {
  url: string;
  method?: NetRequestMethod;
  bodyBase64?: string;
  timeoutMs?: number;
};
export type NetRequestOutput = { status: number; bodyBase64: string };

function parseCachedFileId(value: unknown): string {
  const id = requireString(value, 'attachment fileId');
  if (!/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i.test(id)) {
    throw new Error('Attachment fileId must be a UUID.');
  }
  return id;
}

function parseCachedFileInput(value: unknown): CachedFileInput {
  return { fileId: parseCachedFileId(asObject(value, 'cached attachment input').fileId) };
}

function parsePickedFile(value: unknown, label: string): FileImportFile | CachedFileImport {
  const file = asObject(value, label);
  const name = requireString(file.name, `${label}.name`);
  const thumbnailBase64 = file.thumbnailBase64;
  if (thumbnailBase64 !== undefined && !isAllowedThumbnailBase64(thumbnailBase64)) {
    throw new Error(`${label}.thumbnailBase64 must be a bounded JPEG thumbnail`);
  }
  const preview = thumbnailBase64 === undefined ? {} : { thumbnailBase64 };
  if (Object.hasOwn(file, 'fileId')) {
    if (Object.hasOwn(file, 'base64')) throw new Error('Cached file metadata must not contain the original bytes.');
    const canPreview = optionalBoolean(file.canPreview, `${label}.canPreview`);
    return {
      name, fileId: parseCachedFileId(file.fileId), ...preview,
      size: requireNumber(file.size, `${label}.size`, { min: 0, max: ATTACHMENT_UPLOAD_MAX_BYTES, integer: true }),
      ...(canPreview === undefined ? {} : { canPreview }),
    };
  }
  return { name, base64: requireBase64(file.base64, `${label}.base64`), ...preview };
}

function asObject(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object') {
    throw new Error(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function requireString(value: unknown, label: string, allowEmpty = false): string {
  if (typeof value !== 'string' || (!allowEmpty && value.length === 0)) {
    throw new Error(
      `${label} must be ${allowEmpty ? 'a string' : 'a non-empty string'}`
    );
  }
  return value;
}

function utf8ByteLength(value: string): number {
  let bytes = 0;
  for (const character of value) {
    const code = character.charCodeAt(0);
    if (character.length === 2) bytes += 4;
    else if (code >= 0xd800 && code <= 0xdfff) {
      throw new Error('local storage strings must contain valid Unicode');
    } else if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else bytes += 3;
  }
  return bytes;
}

function parseLocalStorageKey(value: unknown): LocalStorageKeyInput {
  const { key } = asObject(value, 'local storage input');
  const text = requireString(key, 'local storage key', true);
  if (utf8ByteLength(text) > 1024) throw new Error('local storage key is too long');
  return { key: text };
}

function optionalString(
  value: unknown,
  label: string,
  allowEmpty = false
): string | undefined {
  if (value === undefined) return undefined;
  return requireString(value, label, allowEmpty);
}

function parseTTSSpeakInput(value: unknown): TTSSpeakInput {
  if (Array.isArray(value)) throw new Error('tts.speak input must be an object');
  const input = asObject(value, 'tts.speak input');
  const text = requireString(input.text, 'tts.speak input.text');
  if (!text.trim()) throw new Error('tts.speak input.text must be non-blank');
  const locale = optionalString(input.locale, 'tts.speak input.locale');
  if (locale !== undefined && (locale.length > 64 || !/^[a-z]{2,8}(?:-[a-z0-9]{1,8})*$/i.test(locale))) {
    throw new Error('tts.speak input.locale must be a language tag');
  }
  const voice = input.voice === undefined ? 'system' : requireEnum<TTSVoice>(
    input.voice, 'tts.speak input.voice', ['system', 'female', 'male']
  );
  return locale === undefined ? { text, voice } : { text, locale, voice };
}

const NON_BASE64_CHARACTER = /[^A-Za-z0-9+/]/;

/** Upper bound so a Guest cannot overflow the host setTimeout clamp. */
const MAX_SERVICE_TIMEOUT_MS = 300_000;

function requireBase64(value: unknown, label: string): string {
  const text = requireString(value, label, true);
  const padding = text.endsWith('==') ? 2 : Number(text.endsWith('='));
  if (text.length % 4 !== 0 || NON_BASE64_CHARACTER.test(text.slice(0, text.length - padding))) {
    throw new Error(
      `${label} must be standard base64 (RFC 4648, padded)`
    );
  }
  return text;
}

function optionalBase64(value: unknown, label: string): string | undefined {
  if (value === undefined) return undefined;
  return requireBase64(value, label);
}

function requireNumber(
  value: unknown,
  label: string,
  bounds: { min?: number; max?: number; integer?: boolean } = {}
): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`${label} must be a finite number`);
  }
  if (bounds.integer === true && !Number.isInteger(value)) {
    throw new Error(`${label} must be an integer`);
  }
  if (bounds.min !== undefined && value < bounds.min) {
    throw new Error(`${label} must be >= ${bounds.min}`);
  }
  if (bounds.max !== undefined && value > bounds.max) {
    throw new Error(`${label} must be <= ${bounds.max}`);
  }
  return value;
}

function optionalNumber(
  value: unknown,
  label: string,
  bounds?: { min?: number; max?: number; integer?: boolean }
): number | undefined {
  if (value === undefined) return undefined;
  return requireNumber(value, label, bounds);
}

function optionalBoolean(value: unknown, label: string): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== 'boolean') {
    throw new Error(`${label} must be a boolean`);
  }
  return value;
}

function requireStringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} must be an array`);
  }
  return value.map((item, index) => requireString(item, `${label}[${index}]`));
}

function requireNoInput(value: unknown, label: string): void {
  if (value !== undefined) {
    throw new Error(`${label} takes no input`);
  }
}

const INVALID_PATH_CHARS = /[\u0000-\u001f\u007f]/;

/**
 * Package assets use POSIX logical paths. Reject Windows separators, drive or
 * UNC prefixes and `..` segments here; the real file reader must still do a
 * root-containment check.
 */
function requireAssetPath(value: unknown, label: string): string {
  const text = requireString(value, label);
  if (INVALID_PATH_CHARS.test(text)) {
    throw new Error(`${label} must not contain control characters`);
  }
  if (text.includes('\\')) {
    throw new Error(`${label} must use POSIX separators`);
  }
  if (text.includes(':')) {
    throw new Error(`${label} must not contain a drive or scheme prefix`);
  }
  if (text.startsWith('/')) {
    throw new Error(`${label} must be a relative path`);
  }
  if (text.split('/').includes('..')) {
    throw new Error(`${label} must not contain ".." segments`);
  }
  return text;
}

function requireRoutePath(value: unknown, label: string): string {
  const text = requireString(value, label);
  if (!text.startsWith('/')) {
    throw new Error(`${label} must start with "/"`);
  }
  return text;
}

const INVALID_URL_CHARS = /[\s\u0000-\u001f\u007f\\]/;

/**
 * Validate the URL before it crosses the boundary. React Native's `URL`
 * polyfill does not reject malformed input, so the raw-character guard runs
 * first and the hostname/userinfo checks are explicit. Domain allowlists,
 * redirects and SSRF policy remain the broker's job.
 */
function requireUrl(
  value: unknown,
  label: string,
  protocols: readonly string[]
): string {
  const text = requireString(value, label);
  if (INVALID_URL_CHARS.test(text)) {
    throw new Error(
      `${label} must not contain whitespace, control characters, or backslashes`
    );
  }

  let parsed: URL;
  try {
    parsed = new URL(text);
  } catch {
    throw new Error(`${label} must be a valid URL`);
  }

  const protocol = parsed.protocol.endsWith(':')
    ? parsed.protocol.slice(0, -1).toLowerCase()
    : '';
  if (!protocols.includes(protocol)) {
    throw new Error(`${label} must be an ${protocols.join(' or ')} URL`);
  }
  if (parsed.hostname.length === 0) {
    throw new Error(`${label} must include a hostname`);
  }
  if (parsed.username.length > 0 || parsed.password.length > 0) {
    throw new Error(`${label} must not include userinfo`);
  }
  return text;
}

function requireEnum<const T extends string>(
  value: unknown,
  label: string,
  allowed: readonly T[]
): T {
  if (typeof value !== 'string' || !(allowed as readonly string[]).includes(value)) {
    throw new Error(`${label} must be one of ${allowed.join(', ')}`);
  }
  return value as T;
}

export const contract = defineKeelContract({
  version: '1.0.0',
  hostModules: {
    'host:service': {
      call: rpc<ServiceCallInput, ServiceCallOutput>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'service.call input');
            const route = requireString(input.route, 'service.call input.route');
            const payloadBase64 = requireBase64(
              input.payloadBase64,
              'service.call input.payloadBase64'
            );
            const { timeoutMs } = input;
            if (
              timeoutMs !== undefined &&
              (typeof timeoutMs !== 'number' ||
                !Number.isFinite(timeoutMs) ||
                timeoutMs <= 0 ||
                timeoutMs > MAX_SERVICE_TIMEOUT_MS)
            ) {
              throw new Error(
                `service.call input.timeoutMs must be a positive number no greater than ${MAX_SERVICE_TIMEOUT_MS} when present`
              );
            }
            return { route, payloadBase64, timeoutMs };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'service.call output');
            return {
              payloadBase64: requireBase64(
                output.payloadBase64,
                'service.call output.payloadBase64'
              ),
            };
          },
        },
      }),
      tell: rpc<ServiceTellInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'service.tell input');
            return {
              route: requireString(input.route, 'service.tell input.route'),
              payloadBase64: requireBase64(
                input.payloadBase64,
                'service.tell input.payloadBase64'
              ),
            };
          },
        },
      }),
      on: subscription<ServiceEvent>({
        schema: {
          parseEvent: (value) => {
            const event = asObject(value, 'service.on event');
            return {
              event: requireString(event.event, 'service.on event.event'),
              payloadBase64: requireBase64(
                event.payloadBase64,
                'service.on event.payloadBase64'
              ),
            };
          },
        },
      }),
      uploadAttachment: rpc<ServiceUploadAttachmentInput, ServiceUploadAttachmentOutput>({
        schema: {
          parseInput: (value) => {
            const request = asObject(value, 'uploadAttachment input');
            if (Object.hasOwn(request, 'fileId') && request.fileId !== undefined) {
              if (Object.hasOwn(request, 'base64') || Object.hasOwn(request, 'filename')) {
                throw new Error('A cached upload takes a fileId without filename or base64.');
              }
              return {
                fileId: parseCachedFileId(request.fileId),
                type: request.type === undefined ? 'document' : parseAttachmentUploadType(request.type),
                id: optionalString(request.id, 'uploadAttachment input.id'),
              };
            }
            // Shared boundary validation: strict base64, 7 MiB cap, type
            // defaulting to `document`.
            const input = parseAttachmentUploadInput(value);
            return {
              filename: input.filename,
              base64: input.base64,
              type: input.type,
              id: input.id,
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'service.uploadAttachment output');
            return {
              attachmentId: requireString(
                output.attachmentId,
                'service.uploadAttachment output.attachmentId'
              ),
            };
          },
        },
      }),
    },
    'host:service-stream': {
      register: rpc<ServiceStreamInput, void>({ schema: { parseInput: parseServiceStreamInput } }),
      unregister: rpc<ServiceStreamInput, void>({ schema: { parseInput: parseServiceStreamInput } }),
      send: rpc<ServiceStreamSendInput, void>({
        schema: {
          parseInput: (value) => {
            const { streamId } = parseServiceStreamInput(value);
            const input = asObject(value, 'Service DataStream send');
            const route = requireString(input.route, 'Service DataStream route');
            const sequence = requireString(input.sequence, 'Service DataStream sequence');
            if (!/^[1-9][0-9]{0,19}$/.test(sequence) || BigInt(sequence) > 18446744073709551615n) {
              throw new Error('Invalid outbound Service DataStream sequence.');
            }
            const payloadBase64 = requireString(input.payloadBase64, 'Service DataStream payload');
            if (!payloadBase64 || payloadBase64.length > Math.ceil(maxServiceStreamPayload / 3) * 4) {
              throw new Error('Invalid outbound Service DataStream payload size.');
            }
            return { route, streamId, sequence, payloadBase64: requireBase64(payloadBase64, 'Service DataStream payload') };
          },
        },
      }),
      onChunk: subscription<ServiceStreamChunk>({
        schema: {
          parseEvent: (value) => {
            const { streamId } = parseServiceStreamInput(value);
            const chunk = asObject(value, 'Service DataStream chunk');
            const sequence = requireString(chunk.sequence, 'Service DataStream sequence');
            if (!/^(0|[1-9][0-9]{0,19})$/.test(sequence) || BigInt(sequence) > 18446744073709551615n) {
              throw new Error('Invalid Service DataStream sequence.');
            }
            return { streamId, sequence, payloadBase64: requireBase64(chunk.payloadBase64, 'Service DataStream payload') };
          },
        },
      }),
    },
    'host:assets': {
      read: rpc<AssetsReadInput, AssetsReadOutput>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'assets.read input');
            return {
              path: requireAssetPath(input.path, 'assets.read input.path'),
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'assets.read output');
            return {
              base64: requireBase64(
                output.base64,
                'assets.read output.base64'
              ),
            };
          },
        },
      }),
    },
    'host:clipboard': {
      writeText: rpc<ClipboardWriteTextInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'clipboard.writeText input');
            return {
              text: requireString(
                input.text,
                'clipboard.writeText input.text',
                true
              ),
            };
          },
        },
      }),
    },
    'host:tab': {
      setTitle: rpc<TabSetTitleInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'tab.setTitle input');
            const title = requireString(
              input.title,
              'tab.setTitle input.title',
              true
            );
            // Tab titles are shell display strings; bound the UTF-8 byte size
            // so a guest cannot push unbounded data through the title channel.
            if (utf8ByteLength(title) > TAB_TITLE_MAX_BYTES) {
              throw new Error(
                `tab.setTitle input.title must be at most ${TAB_TITLE_MAX_BYTES} UTF-8 bytes`
              );
            }
            return { title };
          },
        },
      }),
    },
    'host:local-storage': {
      getItem: rpc<LocalStorageKeyInput, string | null>({
        schema: {
          parseInput: parseLocalStorageKey,
          parseOutput: (value) =>
            value === null ? null : requireString(value, 'local storage result', true),
        },
      }),
      setItem: rpc<LocalStorageSetInput, void>({
        schema: {
          parseInput: (value) => {
            const { key } = parseLocalStorageKey(value);
            const input = asObject(value, 'local storage input');
            const text = requireString(input.value, 'local storage value', true);
            if (utf8ByteLength(text) > 64 * 1024) throw new Error('local storage value is too large');
            return { key, value: text };
          },
        },
      }),
      removeItem: rpc<LocalStorageKeyInput, void>({
        schema: { parseInput: parseLocalStorageKey },
      }),
    },
    'host:notifications': {
      show: rpc<NotificationsShowInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'notifications.show input');
            return {
              title: requireString(
                input.title,
                'notifications.show input.title'
              ),
              body: optionalString(
                input.body,
                'notifications.show input.body',
                true
              ),
            };
          },
        },
      }),
    },
    'host:camera': {
      capturePhoto: rpc<void, CameraCapturePhotoOutput>({
        schema: {
          parseInput: (value) => requireNoInput(value, 'camera.capturePhoto input'),
          parseOutput: (value) => {
            const output = asObject(value, 'camera.capturePhoto output');
            return {
              base64: requireBase64(
                output.base64,
                'camera.capturePhoto output.base64'
              ),
            };
          },
        },
      }),
    },
    'host:microphone': {
      record: rpc<MicrophoneRecordInput, MicrophoneRecordOutput>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'microphone.record input');
            return {
              durationMs: requireNumber(
                input.durationMs,
                'microphone.record input.durationMs',
                { min: 1, integer: true }
              ),
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'microphone.record output');
            return {
              base64: requireBase64(
                output.base64,
                'microphone.record output.base64'
              ),
            };
          },
        },
      }),
    },
    'host:speech': {
      recognize: rpc<SpeechRecognizeInput, SpeechRecognizeOutput>({
        schema: {
          parseInput: (value) => {
            if (Array.isArray(value)) throw new Error('speech.recognize input must be an object');
            const input = asObject(value, 'speech.recognize input');
            const locale = optionalString(input.locale, 'speech.recognize input.locale');
            if (locale !== undefined && (locale.length > 64 || !/^[a-z]{2,8}(?:-[a-z0-9]{1,8})*$/i.test(locale))) {
              throw new Error('speech.recognize input.locale must be a language tag');
            }
            const continuous = optionalBoolean(input.continuous, 'speech.recognize input.continuous');
            return {
              ...(locale === undefined ? {} : { locale }),
              ...(continuous === undefined ? {} : { continuous }),
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'speech.recognize output');
            return { text: requireString(output.text, 'speech.recognize output.text') };
          },
        },
      }),
      onInterimResult: subscription<SpeechRecognizeOutput>({
        schema: {
          parseEvent: (value) => {
            const result = asObject(value, 'speech.onInterimResult event');
            return { text: requireString(result.text, 'speech.onInterimResult event.text', true) };
          },
        },
      }),
      stop: rpc<void, void>({
        schema: { parseInput: (value) => requireNoInput(value, 'speech.stop input') },
      }),
      onStateChange: subscription<SpeechRecordingState>({
        schema: {
          parseEvent: (value) => ({
            state: requireEnum(asObject(value, 'speech.onStateChange event').state,
              'speech.onStateChange event.state', ['recording', 'finalizing']),
          }),
        },
      }),
    },
    'host:tts': {
      speak: rpc<TTSSpeakInput, TTSSpeakOutput>({
        schema: {
          parseInput: parseTTSSpeakInput,
          parseOutput: (value) => {
            const { status } = asObject(value, 'tts.speak output');
            return { status: requireEnum(status, 'tts.speak output.status', ['finished', 'cancelled']) };
          },
        },
      }),
      stop: rpc<void, void>({
        schema: { parseInput: (value) => requireNoInput(value, 'tts.stop input') },
      }),
    },
    'host:location': {
      getCurrentPosition: rpc<void, LocationPositionOutput>({
        schema: {
          parseInput: (value) =>
            requireNoInput(value, 'location.getCurrentPosition input'),
          parseOutput: (value) => {
            const output = asObject(value, 'location.getCurrentPosition output');
            return {
              latitude: requireNumber(
                output.latitude,
                'location.getCurrentPosition output.latitude',
                { min: -90, max: 90 }
              ),
              longitude: requireNumber(
                output.longitude,
                'location.getCurrentPosition output.longitude',
                { min: -180, max: 180 }
              ),
              accuracy: optionalNumber(
                output.accuracy,
                'location.getCurrentPosition output.accuracy',
                { min: 0 }
              ),
            };
          },
        },
      }),
    },
    'host:external-link': {
      open: rpc<ExternalLinkOpenInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'external-link.open input');
            return {
              url: requireUrl(
                input.url,
                'external-link.open input.url',
                ['http', 'https']
              ),
            };
          },
        },
      }),
    },
    'host:navigate': {
      navigate: rpc<NavigateInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'navigate.navigate input');
            return {
              path: requireRoutePath(
                input.path,
                'navigate.navigate input.path'
              ),
              mode:
                input.mode === undefined
                  ? undefined
                  : requireEnum(
                      input.mode,
                      'navigate.navigate input.mode',
                      ['push', 'replace'] as const
                    ),
            };
          },
        },
      }),
    },
    'host:file-import': {
      pick: rpc<FileImportPickInput, FileImportPickOutput>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'file-import.pick input');
            return {
              source: input.source === undefined ? undefined : requireEnum(
                input.source, 'file-import.pick input.source', ['files', 'photos'] as const
              ),
              accept:
                input.accept === undefined
                  ? undefined
                  : requireStringArray(
                      input.accept,
                      'file-import.pick input.accept'
                    ),
              multiple: optionalBoolean(
                input.multiple,
                'file-import.pick input.multiple'
              ),
              ...(input.cache === undefined ? {} : { cache: optionalBoolean(input.cache, 'file-import.pick input.cache') }),
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'file-import.pick output');
            if (!Array.isArray(output.files)) {
              throw new Error('file-import.pick output.files must be an array');
            }
            return {
              files: output.files.map((item, index) => parsePickedFile(item, `file-import.pick output.files[${index}]`)),
            };
          },
        },
      }),
      openPreview: rpc<CachedFileInput, void>({ schema: { parseInput: parseCachedFileInput } }),
      // Releasing an already absent file in the active document is idempotent.
      release: rpc<CachedFileInput, void>({ schema: { parseInput: parseCachedFileInput } }),
    },
    'host:file-export': {
      save: rpc<FileExportSaveInput, FileExportSaveOutput>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'file-export.save input');
            return {
              name: requireString(input.name, 'file-export.save input.name'),
              base64: requireBase64(
                input.base64,
                'file-export.save input.base64'
              ),
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'file-export.save output');
            if (typeof output.saved !== 'boolean') {
              throw new Error('file-export.save output.saved must be a boolean');
            }
            return { saved: output.saved };
          },
        },
      }),
    },
    'host:share': {
      share: rpc<ShareInput, void>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'share.share input');
            const text = optionalString(input.text, 'share.share input.text');
            const url = optionalString(input.url, 'share.share input.url');
            if (text === undefined && url === undefined) {
              throw new Error('share.share input requires text or url');
            }
            return { text, url };
          },
        },
      }),
    },
    'host:net': {
      request: rpc<NetRequestInput, NetRequestOutput>({
        schema: {
          parseInput: (value) => {
            const input = asObject(value, 'net.request input');
            const method =
              input.method === undefined
                ? undefined
                : requireEnum(input.method, 'net.request input.method', [
                    'GET',
                    'POST',
                  ] as const);
            const bodyBase64 = optionalBase64(
              input.bodyBase64,
              'net.request input.bodyBase64'
            );
            // An absent method defaults to GET, so a body must name POST
            // explicitly rather than relying on the caller's interpretation.
            if (bodyBase64 !== undefined && method !== 'POST') {
              throw new Error(
                'net.request input.bodyBase64 requires an explicit POST method'
              );
            }
            return {
              url: requireUrl(input.url, 'net.request input.url', ['https']),
              method,
              bodyBase64,
              timeoutMs: optionalNumber(
                input.timeoutMs,
                'net.request input.timeoutMs',
                { min: 1 }
              ),
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'net.request output');
            return {
              status: requireNumber(
                output.status,
                'net.request output.status',
                { min: 100, max: 599, integer: true }
              ),
              bodyBase64: requireBase64(
                output.bodyBase64,
                'net.request output.bodyBase64'
              ),
            };
          },
        },
      }),
    },
  },
  guestExports: {},
});

export type ActriumContract = typeof contract;
