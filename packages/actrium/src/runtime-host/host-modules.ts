import type { HostModulesImplementation } from 'keel/contract';

import type {
  ActriumContract,
  ClipboardWriteTextInput,
  LocalStorageKeyInput,
  LocalStorageSetInput,
  FileImportPickInput,
  FileImportPickOutput,
  ServiceCallInput,
  ServiceEvent,
  ServiceTellInput,
} from './contract';
import type { FrontCapability } from './capabilities';
import type { LocalStorageCall } from './local-storage';

export type ServiceCallRequest = ServiceCallInput;

export type ServiceCallResult = {
  success: boolean;
  payloadBase64?: string;
  message?: string;
};

export type ActorCall = (request: {
  requestId: string;
  route: string;
  payloadBase64: string;
  timeoutMs: number;
}) => Promise<ServiceCallResult>;

/**
 * The implementation map is derived from the contract, so a module that is
 * declared there cannot be skipped here (TypeScript and the runtime
 * `assertImplementationMatchesContract` both enforce it).
 */
export type HostModules = HostModulesImplementation<ActriumContract>;

/** Methods with a real Host implementation, excluding throwing placeholders. */
export const HOST_SUPPORTED_METHODS: Readonly<Record<string, ReadonlySet<string>>> = {
  'host:service': new Set(['call']),
  'host:clipboard': new Set(['writeText']),
  'host:local-storage': new Set(['getItem', 'setItem', 'removeItem']),
  'host:file-import': new Set(['pick']),
};

/** Modules with a real Host implementation, not just a contract declaration. */
export const HOST_SUPPORTED_MODULES: ReadonlySet<string> = new Set(
  Object.keys(HOST_SUPPORTED_METHODS)
);

function notImplemented(moduleId: string, exportName: string): Error {
  return new Error(`${moduleId}.${exportName} is not implemented`);
}

// `callActor` is expected to resolve with a failure result instead of
// rejecting after the race settles. HostHttpHandler catches transport errors,
// so a late rejection cannot surface as an unhandled rejection here.
function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`Service call timed out after ${timeoutMs} ms`)),
      timeoutMs
    );
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer !== undefined) clearTimeout(timer);
  });
}

/**
 * Host-side implementations for the Guest contract. Keel validates this map
 * against the contract at Engine construction time, so a missing capability
 * fails closed before the Guest bundle runs.
 *
 * Placeholder methods throw and stay out of the supported capability sets.
 */
export function createHostModules(options: {
  hasServiceCapability: boolean;
  callActor: ActorCall;
  authorizeCapability: (capability: FrontCapability) => Promise<void>;
  writeClipboardText: (text: string) => Promise<void>;
  localStorage?: LocalStorageCall;
  pickFiles?: (input: FileImportPickInput) => Promise<FileImportPickOutput>;
}): HostModules {
  let requestSequence = 0;
  const localStorage: LocalStorageCall = async (operation, input) => {
    await options.authorizeCapability(`host:local-storage.${operation}`);
    if (!options.localStorage) throw new Error('The host local storage module is unavailable.');
    return options.localStorage(operation, input);
  };

  return {
    'host:service': {
      call: async (input: ServiceCallInput) => {
        if (!options.hasServiceCapability) {
          throw new Error(
            'The application does not declare the service capability.'
          );
        }
        requestSequence += 1;
        const timeoutMs = input.timeoutMs ?? 30_000;
        const result = await withTimeout(
          options.callActor({
            requestId: `service-${Date.now()}-${requestSequence}`,
            route: input.route,
            payloadBase64: input.payloadBase64,
            timeoutMs,
          }),
          timeoutMs
        );
        if (!result.success || typeof result.payloadBase64 !== 'string') {
          throw new Error(result.message ?? 'Service call failed');
        }
        return { payloadBase64: result.payloadBase64 };
      },
      tell: async (_input: ServiceTellInput) => {
        throw notImplemented('host:service', 'tell');
      },
      on: (_handler: (event: ServiceEvent) => void) => {
        throw notImplemented('host:service', 'on');
      },
    },
    'host:assets': {
      read: async () => {
        throw notImplemented('host:assets', 'read');
      },
    },
    'host:clipboard': {
      writeText: async (input: ClipboardWriteTextInput) => {
        await options.authorizeCapability('host:clipboard.write');
        await options.writeClipboardText(input.text);
      },
    },
    'host:local-storage': {
      getItem: (input: LocalStorageKeyInput) => localStorage('getItem', input),
      setItem: async (input: LocalStorageSetInput) => {
        await localStorage('setItem', input);
      },
      removeItem: async (input: LocalStorageKeyInput) => {
        await localStorage('removeItem', input);
      },
    },
    'host:notifications': {
      show: async () => {
        throw notImplemented('host:notifications', 'show');
      },
    },
    'host:camera': {
      capturePhoto: async () => {
        throw notImplemented('host:camera', 'capturePhoto');
      },
    },
    'host:microphone': {
      record: async () => {
        throw notImplemented('host:microphone', 'record');
      },
    },
    'host:location': {
      getCurrentPosition: async () => {
        throw notImplemented('host:location', 'getCurrentPosition');
      },
    },
    'host:external-link': {
      open: async () => {
        throw notImplemented('host:external-link', 'open');
      },
    },
    'host:navigate': {
      navigate: async () => {
        throw notImplemented('host:navigate', 'navigate');
      },
    },
    'host:file-import': {
      pick: async (input) => {
        if (!options.pickFiles) throw new Error('The host file picker is unavailable.');
        await options.authorizeCapability('host:file-import.pick');
        return options.pickFiles(input);
      },
    },
    'host:file-export': {
      save: async () => {
        throw notImplemented('host:file-export', 'save');
      },
    },
    'host:share': {
      share: async () => {
        throw notImplemented('host:share', 'share');
      },
    },
    'host:net': {
      request: async () => {
        throw notImplemented('host:net', 'request');
      },
    },
  };
}
