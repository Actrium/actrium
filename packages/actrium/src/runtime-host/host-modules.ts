import type { HostModulesImplementation } from 'keel/contract';

import type {
  ActriumContract,
  ClipboardWriteTextInput,
  TabSetTitleInput,
  LocalStorageKeyInput,
  LocalStorageSetInput,
  FileImportPickInput,
  FileImportPickOutput,
  ServiceCallInput,
  ServiceEvent,
  ServiceStreamSendInput,
  ServiceTellInput,
  ServiceUploadAttachmentInput,
  ServiceUploadAttachmentOutput,
  SpeechInterimHandler,
  SpeechRecognizeInput,
  SpeechRecognizeOutput,
  SpeechStateHandler,
} from './contract';
import type { FrontCapability } from './capabilities';
import type { LocalStorageCall } from './local-storage';
import type { ServiceEventSource } from './service-events';
import type { NativeServiceStreams } from './service-streams';
import type { AttachmentUploadManager } from './attachment-upload';
import type { GuestTTS } from './guest-tts';

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
  'host:service': new Set(['call', 'on', 'uploadAttachment']),
  'host:service-stream': new Set(['register', 'unregister', 'onChunk', 'send']),
  'host:clipboard': new Set(['writeText']),
  'host:tab': new Set(['setTitle']),
  'host:local-storage': new Set(['getItem', 'setItem', 'removeItem']),
  'host:speech': new Set(['recognize', 'onInterimResult', 'stop', 'onStateChange']),
  'host:tts': new Set(['speak', 'stop']),
  'host:file-import': new Set(['pick', 'openPreview', 'release']),
  'host:file-export': new Set(['save']),
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
  hasServiceOnCapability: boolean;
  callActor: ActorCall;
  serviceEventSource?: ServiceEventSource;
  serviceStreams?: NativeServiceStreams;
  sendServiceStreamChunk?: (input: ServiceStreamSendInput) => Promise<void>;
  authorizeCapability: (capability: FrontCapability) => Promise<void>;
  onCapabilityRevoked?: (
    capability: FrontCapability,
    listener: () => void
  ) => () => void;
  /** Track the complete Guest cleanup for engine reload/tab teardown. */
  trackServiceEventSubscription?: (cleanup: () => void) => () => void;
  writeClipboardText: (text: string) => Promise<void>;
  /** Runtime tab title update, analogous to `document.title`. */
  setTabTitle?: (title: string) => void;
  localStorage?: LocalStorageCall;
  pickFiles?: (input: FileImportPickInput) => Promise<FileImportPickOutput>;
  saveFile?: HostModules['host:file-export']['save'];
  openFilePreview?: (fileId: string) => Promise<void>;
  releaseCachedFile?: (fileId: string) => Promise<void>;
  readCachedFile?: (fileId: string) => Promise<{ filename: string; base64: string }>;
  recognizeSpeech?: (
    input: SpeechRecognizeInput, onInterim?: SpeechInterimHandler, onState?: SpeechStateHandler
  ) => Promise<SpeechRecognizeOutput>;
  stopSpeech?: () => Promise<void>;
  tts?: GuestTTS;
  /**
   * Fixed transport attachment uploader. When absent, `uploadAttachment`
   * fails closed; there is deliberately no HTTP or legacy fallback.
   */
  attachmentUploader?: AttachmentUploadManager;
}): HostModules {
  let requestSequence = 0;
  const localStorage: LocalStorageCall = async (operation, input) => {
    await options.authorizeCapability(`host:local-storage.${operation}`);
    if (!options.localStorage) throw new Error('The host local storage module is unavailable.');
    return options.localStorage(operation, input);
  };
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
      on: (handler: (event: ServiceEvent) => void) => {
        if (!options.hasServiceOnCapability) {
          throw new Error(
            'The application does not declare the service.on capability.'
          );
        }
        if (!options.serviceEventSource) {
          throw new Error('Service event source is not available.');
        }

        // Authorize before opening the native or HTTP transport. The cleanup
        // handle remains synchronous and also cancels a pending authorization.
        let disposed = false;
        let unsubscribeTransport: (() => void) | undefined;
        let stopWatchingRevocation: (() => void) | undefined;
        let stopTracking: (() => void) | undefined;
        const cleanup = () => {
          if (disposed) return;
          disposed = true;
          stopWatchingRevocation?.();
          stopWatchingRevocation = undefined;
          stopTracking?.();
          stopTracking = undefined;
          unsubscribeTransport?.();
          unsubscribeTransport = undefined;
        };
        const untrack = options.trackServiceEventSubscription?.(cleanup);
        if (disposed) untrack?.();
        else stopTracking = untrack;
        void options.authorizeCapability('service.on').then(
          () => {
            if (disposed) return;
            // Attach revocation before startup. `onCapabilityRevoked` checks
            // the latest snapshot synchronously, so a revoke that landed while
            // authorization was resolving cannot open the transport.
            const stopWatching = options.onCapabilityRevoked?.('service.on', cleanup);
            if (disposed) {
              stopWatching?.();
              return;
            }
            stopWatchingRevocation = stopWatching;
            try {
              unsubscribeTransport = options.serviceEventSource!.subscribe((event) => {
                if (disposed) return;
                try {
                  handler(event);
                } catch (error) {
                  console.warn('[Actrium] Service event handler failed:', error);
                }
              });
              if (disposed) {
                unsubscribeTransport?.();
                unsubscribeTransport = undefined;
              }
            } catch (error) {
              console.warn('[Actrium] Service event subscription failed:', error);
              cleanup();
            }
          },
          (error) => {
            console.warn('[Actrium] service.on authorization rejected:', error);
            cleanup();
          }
        );

        return cleanup;
      },
      uploadAttachment: async (
        input: ServiceUploadAttachmentInput
      ): Promise<ServiceUploadAttachmentOutput> => {
        if (!options.attachmentUploader) {
          throw new Error('The native attachment upload bridge is unavailable.');
        }
        await options.authorizeCapability('service.attachmentUpload');
        if (input.fileId !== undefined) {
          if (!options.readCachedFile) throw new Error('The attachment cache is unavailable.');
          const file = await options.readCachedFile(input.fileId);
          return options.attachmentUploader.upload({ ...file, type: input.type, id: input.id });
        }
        return options.attachmentUploader.upload(input);
      },
    },
    'host:service-stream': {
      register: async ({ streamId }) => {
        await options.authorizeCapability('host:service-stream.register');
        if (!options.serviceStreams) throw new Error('The native Service DataStream bridge is unavailable.');
        await options.serviceStreams.register(streamId);
      },
      unregister: async ({ streamId }) => {
        options.serviceStreams?.unregister(streamId);
      },
      send: async (input) => {
        await options.authorizeCapability('host:service-stream.send');
        if (!options.sendServiceStreamChunk) throw new Error('Service DataStream sending is unavailable.');
        await options.sendServiceStreamChunk(input);
      },
      onChunk: (handler) => options.serviceStreams?.subscribe(handler) ?? (() => {}),
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
    'host:tab': {
      setTitle: async (input: TabSetTitleInput) => {
        await options.authorizeCapability('host:tab.setTitle');
        if (!options.setTabTitle) {
          throw new Error('The host tab title module is unavailable.');
        }
        options.setTabTitle(input.title);
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
    'host:tts': {
      speak: async (input) => {
        if (!options.tts) throw new Error('Text-to-speech is unavailable.');
        return options.tts.speak(input);
      },
      stop: async () => {
        if (!options.tts) throw new Error('Text-to-speech is unavailable.');
        await options.tts.stop();
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
        const selected = await options.pickFiles(input);
        if (selected.files.some((file) => ('fileId' in file) !== Boolean(input.cache))) {
          throw new Error('The file picker returned a different attachment representation.');
        }
        return selected;
      },
      openPreview: async ({ fileId }) => {
        await options.authorizeCapability('host:file-import.openPreview');
        if (!options.openFilePreview) throw new Error('The attachment preview is unavailable.');
        await options.openFilePreview(fileId);
      },
      release: async ({ fileId }) => {
        await options.authorizeCapability('host:file-import.release');
        if (!options.releaseCachedFile) throw new Error('The attachment cache is unavailable.');
        await options.releaseCachedFile(fileId);
      },
    },
    'host:file-export': {
      save: async (input) => {
        await options.authorizeCapability('host:file-export.save');
        if (!options.saveFile) throw new Error('The host file export module is unavailable.');
        return options.saveFile(input);
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
