import type { HostModulesImplementation } from 'keel/contract';

import type {
  ActriumContract,
  ClipboardWriteTextInput,
  FileImportPickInput,
  FileImportPickOutput,
  ServiceCallInput,
  ServiceEvent,
  ServiceTellInput,
  SpeechInterimHandler,
  SpeechRecognizeInput,
  SpeechRecognizeOutput,
  SpeechStateHandler,
} from './contract';
import type { FrontCapability } from './capabilities';

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
  'host:speech': new Set(['recognize', 'onInterimResult', 'stop', 'onStateChange']),
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
 * Capabilities without an implementation expose placeholders that throw
 * `host:<name>.<method> is not implemented` instead of returning `undefined`,
 * so a call cannot be mistaken for success. Placeholder capabilities also stay
 * out of `HOST_SUPPORTED_CAPABILITIES`.
 */
export function createHostModules(options: {
  hasServiceCapability: boolean;
  callActor: ActorCall;
  authorizeCapability: (capability: FrontCapability) => Promise<void>;
  writeClipboardText: (text: string) => Promise<void>;
  pickFiles?: (input: FileImportPickInput) => Promise<FileImportPickOutput>;
  recognizeSpeech?: (
    input: SpeechRecognizeInput, onInterim?: SpeechInterimHandler, onState?: SpeechStateHandler
  ) => Promise<SpeechRecognizeOutput>;
  stopSpeech?: () => Promise<void>;
}): HostModules {
  let requestSequence = 0;
  let cancelSpeechAuthorization: (() => void) | undefined;
  const speechListeners = new Set<SpeechInterimHandler>();
  const speechStateListeners = new Set<SpeechStateHandler>();
  const emitSpeechInterim: SpeechInterimHandler = (result) => {
    for (const listener of speechListeners) listener(result);
  };
  const emitSpeechState: SpeechStateHandler = (state) => {
    for (const listener of speechStateListeners) listener(state);
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
    'host:speech': {
      recognize: async (input) => {
        if (!options.recognizeSpeech) throw new Error('Speech recognition is unavailable.');
        if (cancelSpeechAuthorization) throw new Error('Speech recognition is already in progress.');
        const cancellation = new Error('Speech recognition was cancelled.');
        let cancel!: () => void;
        const interrupted = new Promise<never>((_, reject) => { cancel = () => reject(cancellation); });
        cancelSpeechAuthorization = cancel;
        try {
          await Promise.race([options.authorizeCapability('host:speech.recognize'), interrupted]);
          if (cancelSpeechAuthorization !== cancel) throw cancellation;
        } finally {
          if (cancelSpeechAuthorization === cancel) cancelSpeechAuthorization = undefined;
        }
        return options.recognizeSpeech(input, speechListeners.size ? emitSpeechInterim : undefined, emitSpeechState);
      },
      onInterimResult: (handler) => {
        speechListeners.add(handler);
        return () => { speechListeners.delete(handler); };
      },
      stop: () => {
        const cancel = cancelSpeechAuthorization;
        if (!cancel) return options.stopSpeech?.();
        cancelSpeechAuthorization = undefined;
        cancel();
      },
      onStateChange: (handler) => {
        speechStateListeners.add(handler);
        return () => { speechStateListeners.delete(handler); };
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
