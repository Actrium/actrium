import { defineKeelContract, rpc, subscription } from 'keel/contract';

/**
 * Host capabilities declared to Guests (v1 runtime spec, section 5.2/5.3).
 *
 * `host:service` is the only implemented module today. The other twelve
 * modules are declared so builds accept their imports and the Host-side
 * boundary schemas are defined, but every method is a fail-closed placeholder:
 * the Host implementation throws `host:<name>.<method> is not implemented` on
 * call. Guest-side TypeScript declarations for these modules are not generated
 * yet; see TODO M2-N6.
 * Declaring a module here does NOT make it available in the effective
 * capability set; see `HOST_SUPPORTED_CAPABILITIES` in `capability-model.ts`.
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

export type AssetsReadInput = { path: string };
export type AssetsReadOutput = { base64: string };

export type ClipboardWriteTextInput = { text: string };

export type NotificationsShowInput = { title: string; body?: string };

export type CameraCapturePhotoOutput = { base64: string };

export type MicrophoneRecordInput = { durationMs: number };
export type MicrophoneRecordOutput = { base64: string };

export type LocationPositionOutput = {
  latitude: number;
  longitude: number;
  accuracy?: number;
};

export type ExternalLinkOpenInput = { url: string };

export type NavigateInput = { path: string; mode?: 'push' | 'replace' };

export type FileImportFile = { name: string; base64: string };
export type FileImportPickInput = { accept?: string[]; multiple?: boolean };
export type FileImportPickOutput = { files: FileImportFile[] };

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

function optionalString(
  value: unknown,
  label: string,
  allowEmpty = false
): string | undefined {
  if (value === undefined) return undefined;
  return requireString(value, label, allowEmpty);
}

const BASE64_PATTERN =
  /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

/** Upper bound so a Guest cannot overflow the host setTimeout clamp. */
const MAX_SERVICE_TIMEOUT_MS = 300_000;

function requireBase64(value: unknown, label: string): string {
  const text = requireString(value, label, true);
  if (!BASE64_PATTERN.test(text)) {
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
            };
          },
          parseOutput: (value) => {
            const output = asObject(value, 'file-import.pick output');
            if (!Array.isArray(output.files)) {
              throw new Error('file-import.pick output.files must be an array');
            }
            return {
              files: output.files.map((item, index) => {
                const label = `file-import.pick output.files[${index}]`;
                const file = asObject(item, label);
                return {
                  name: requireString(file.name, `${label}.name`),
                  base64: requireBase64(file.base64, `${label}.base64`),
                };
              }),
            };
          },
        },
      }),
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
