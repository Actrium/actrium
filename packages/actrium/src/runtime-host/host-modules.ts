import type { HostModulesImplementation } from 'keel/contract';

import type {
  ActriumContract,
  ClipboardWriteTextInput,
  ServiceCallInput,
  ServiceEvent,
  ServiceTellInput,
} from './contract';

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
 * `host:service` and `host:clipboard.write` are implemented. Every other
 * module is a deliberate placeholder: its methods throw
 * `host:<name>.<method> is not implemented` instead of returning `undefined`,
 * so a call cannot be mistaken for success. Placeholder capabilities also stay
 * out of `HOST_SUPPORTED_CAPABILITIES`.
 */
export function createHostModules(options: {
  hasServiceCapability: boolean;
  callActor: ActorCall;
  hasClipboardCapability: boolean;
  /**
   * User grant for `host:clipboard.write`. Declaration alone is not
   * authorization: the capability requires a user grant, so callers must
   * pass the granted/effective state explicitly.
   */
  hasClipboardGrant: boolean;
  writeClipboardText: (text: string) => Promise<void>;
}): HostModules {
  let requestSequence = 0;

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
        if (!options.hasClipboardCapability) {
          throw new Error(
            'The application does not declare the host:clipboard.write capability.'
          );
        }
        if (!options.hasClipboardGrant) {
          throw new Error(
            'The application has not been granted the host:clipboard.write capability.'
          );
        }
        await options.writeClipboardText(input.text);
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
      pick: async () => {
        throw notImplemented('host:file-import', 'pick');
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
