import React from 'react';
import {
  ActivityIndicator as NativeActivityIndicator,
  DeviceEventEmitter,
  Image as NativeImage,
  NativeEventEmitter,
  NativeModules,
  Pressable as NativePressable,
  Text as NativeText,
  type ViewStyle,
  View as NativeView,
} from 'react-native';
import { EventHandler } from 'askit/host-events';
import { Engine, type ComponentMap } from 'keel/host';
import {
  FlatList,
  ScrollView,
  Switch,
  Text,
  View,
} from 'keel/host/preset';

import { GuestTextInput } from './GuestTextInput';
import { hasDeclaredCapability } from './capabilities';
import {
  capabilitiesFromSnapshotJSON,
  CapabilityAuthorizer,
  evaluateCapabilityAccess,
} from './capability-authorization';
import {
  contract, type FileExportSaveInput, type FileExportSaveOutput, type FileImportPickInput, type FileImportPickOutput,
} from './contract';
import {
  AttachmentUploadManager,
  createRealmServiceAttachmentAdapter,
  type RealmServiceAttachmentBridge,
} from './attachment-upload';
import { createHostModules, type ActorCall } from './host-modules';
import { createGuestMenuComponents, guestMenuNames } from './guest-menu';
import { GuestTTS, type GuestTTSBridge } from './guest-tts';
import { isAllowedImageSource, pickGuestImageProps } from './image-asset';
import { bindNativeLocalStorage } from './local-storage';
import { Supramark } from './guest-markdown-renderer';
import { Svg } from './guest-svg-renderer';
import { DocumentSelectionSurface } from './DocumentSelectionSurface';
import { CachedImagePreview } from './CachedImagePreview';
import { verifyRuntimeCapabilityManifestJSON } from './runtime-capability-verify';
import { callHttpService, callNativeService } from './service-call';
import { NativeServiceStreams } from './service-streams';
import {
  createHttpServiceEventSource,
  createNativeServiceEventSource,
  serviceEventsFromManifest,
  type CapabilityManifestV1,
  type ServiceEventSource,
} from './service-events';
import { bindV1HostModules, verifyV1RuntimeGuest } from './v1-guest';
import type { TabConfig } from './types';
import { isGuestSpeechAvailable, useGuestSpeech } from './use-guest-speech';

type GuestButtonProps = {
  title: string;
  onPress?: () => void;
  color?: string;
  disabled?: boolean;
  style?: ViewStyle;
  children?: React.ReactNode;
  accessibilityLabel?: string;
  testID?: string;
};

const GuestButton: React.FC<GuestButtonProps> = ({
  title,
  onPress,
  color = '#2563eb',
  disabled = false,
  style,
  children,
  accessibilityLabel,
  testID,
}) => (
  <NativePressable
    accessibilityRole='button'
    accessibilityLabel={accessibilityLabel ?? title}
    testID={testID}
    disabled={disabled}
    onPress={() => onPress?.()}
    style={({ pressed }) => [{
      alignItems: 'center',
      backgroundColor: disabled ? '#31527f' : color,
      borderRadius: 10,
      padding: 14,
    }, style, pressed && { opacity: 0.8 }]}
  >
    {children ?? <NativeText style={{ color: '#ffffff', fontSize: 16, fontWeight: '600' }}>
      {title}
    </NativeText>}
  </NativePressable>
);

const GuestImage: React.FC<React.ComponentProps<typeof NativeImage>> = (
  props
) => {
  if (!isAllowedImageSource(props.source)) {
    return null;
  }
  return (
    <NativeImage
      {...(pickGuestImageProps(props as Record<string, unknown>) as React.ComponentProps<typeof NativeImage>)}
    />
  );
};

const hostComponents: ComponentMap = {
  Button: GuestButton as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  FlatList: FlatList as unknown as React.ComponentType<Record<string, unknown>>,
  // v1 exposes images as `ImageAsset`; the guest SDK still emits `Image`.
  Image: GuestImage as unknown as React.ComponentType<Record<string, unknown>>,
  ImageAsset: GuestImage as unknown as React.ComponentType<
    Record<string, unknown>
  >,
  ...(Supramark && { Supramark }),
  ...(Svg && { Svg: Svg as unknown as ComponentMap[string] }),
  ScrollView: ScrollView as React.ComponentType<Record<string, unknown>>,
  Switch: Switch as unknown as React.ComponentType<Record<string, unknown>>,
  Text: Text as React.ComponentType<Record<string, unknown>>,
  TextInput: GuestTextInput as React.ComponentType<Record<string, unknown>>,
  View: View as React.ComponentType<Record<string, unknown>>,
};

type Props = {
  tab: TabConfig;
  isOverviewMode?: boolean;
  containerWidth?: number;
  containerHeight?: number;
};

const logToNative = (message: string) => {
  console.log(message);
  try {
    NativeModules.NativeLogger?.log?.(message);
  } catch {
    // Logging must never affect guest execution.
  }
};

/**
 * RN's Clipboard JS wrapper is deprecated; call the native module directly.
 * RCTClipboard is part of react-native-macos and exports `setString`.
 */
const setClipboardText = (text: string): void => {
  const clipboard = NativeModules.Clipboard as
    | { setString?: (value: string) => void }
    | undefined;
  if (!clipboard?.setString) {
    throw new Error('The host clipboard module is not available.');
  }
  clipboard.setString(text);
};

const writeClipboardText = async (text: string): Promise<void> => setClipboardText(text);

type TabTitleBridge = {
  setTitle?: (title: string, tabId: string, reloadKey: number) => void;
};

/** Runtime tab title update, analogous to `document.title`. */
const setTabTitle = (tabId: string, reloadKey: number, title: string): void => {
  const bridge = NativeModules.TabTitle as TabTitleBridge | undefined;
  if (!bridge?.setTitle) {
    throw new Error('The host tab title bridge is not available.');
  }
  bridge.setTitle(title, tabId, reloadKey);
};

type CapabilityPermissionBridge = {
  requestGrant?: (tabId: string, capability: string) => void;
};

type NativePick = (
  tabId: string, reloadKey: number, source: NonNullable<FileImportPickInput['source']>,
  accept: string[], multiple: boolean
) => Promise<FileImportPickOutput>;
type CachedFileResult = { read: { filename: string; base64: string }; preview: string; release: void };
type FileImportBridge = {
  pick?: NativePick;
  pickCached?: NativePick;
  cachedAttachment?<Action extends keyof CachedFileResult>(
    tabId: string, reloadKey: number, action: Action, fileId: string
  ): Promise<CachedFileResult[Action]>;
};
type FileExportBridge = {
  save?(tabId: string, reloadKey: number, name: string, base64: string): Promise<FileExportSaveOutput>;
};

const requestNativeCapabilityGrant = (
  tabId: string,
  capability: string
): void => {
  const bridge = NativeModules.CapabilityPermission as
    | CapabilityPermissionBridge
    | undefined;
  if (!bridge?.requestGrant) {
    throw new Error('The native capability permission bridge is not available.');
  }
  bridge.requestGrant(tabId, capability);
};

export const StandaloneGuestApp: React.FC<Props> = ({
  tab,
  isOverviewMode,
  containerWidth,
  containerHeight,
}) => {
  const [content, setContent] = React.useState<React.ReactElement | null>(null);
  const [error, setError] = React.useState('');
  const [preview, setPreview] = React.useState<{ fileId: string; uri: string } | null>(null);
  const guestScript = tab.unifiedScript || tab.guestScript;
  const hasServiceCapability = hasDeclaredCapability(
    tab.permissions ?? [],
    'service.call'
  );
  const hasServiceOnCapability = hasDeclaredCapability(
    tab.permissions ?? [],
    'service.on'
  );
  const permissionsKey = [...(tab.permissions ?? [])].sort().join(',');
  const capabilitySnapshot = React.useMemo(
    () =>
      tab.capabilitySnapshotJSON
        ? capabilitiesFromSnapshotJSON(tab.capabilitySnapshotJSON)
        : null,
    [tab.capabilitySnapshotJSON]
  );
  const capabilitySnapshotAvailable = capabilitySnapshot !== null;
  const { active: speechActive, request: recognizeSpeech, stop: stopSpeech, cancel: cancelSpeech } = useGuestSpeech(
    isOverviewMode === false && capabilitySnapshot !== null &&
      evaluateCapabilityAccess('host:speech.recognize', capabilitySnapshot).allowed
  );
  const authorizerRef = React.useRef<CapabilityAuthorizer | null>(null);
  if (!authorizerRef.current && capabilitySnapshot) {
    authorizerRef.current = new CapabilityAuthorizer(
      capabilitySnapshot,
      (capability) => requestNativeCapabilityGrant(tab.id, capability)
    );
  }

  React.useEffect(() => {
    if (capabilitySnapshot) {
      authorizerRef.current?.update(capabilitySnapshot);
    }
  }, [capabilitySnapshot]);

  React.useEffect(
    () => () => {
      authorizerRef.current?.dispose();
    },
    [tab.id]
  );

  React.useEffect(() => {
    // A reload must start from a clean state, otherwise a previous load
    // failure keeps rendering even after the new bundle loads successfully.
    setError('');
    setContent(null);
    setPreview(null);

    if (!guestScript) {
      setError('The application does not contain a Keel guest bundle.');
      return;
    }
    const capabilityManifestJSON = tab.capabilityManifestJSON;
    if (!capabilityManifestJSON) {
      setError('The package does not contain a capability manifest.');
      return;
    }
    // Only the Host-designated Echo demo may use the earlier Guest bundle format.
    const legacyEchoTab = tab.legacyActorBridge === true;
    const v1 = legacyEchoTab
      ? undefined
      : verifyV1RuntimeGuest(guestScript, capabilityManifestJSON, tab.permissions ?? []);
    const verification = v1 ?? verifyRuntimeCapabilityManifestJSON(
      capabilityManifestJSON,
      tab.permissions ?? []
    );
    if (!verification.ok) {
      const details = verification.details.join(' ');
      logToNative(
        `[StandaloneGuestApp] capability verification failed: ${details}`
      );
      setError(`Capability manifest verification failed: ${details}`);
      return;
    }
    if (v1?.ok && v1.manifest.components.includes('Supramark') && !Supramark) {
      setError('Supramark rendering is unavailable in this Host build.');
      return;
    }
    if (v1?.ok && v1.manifest.components.includes('Svg') && !Svg) {
      setError('SVG rendering is unavailable in this Host build.');
      return;
    }
    const menus = createGuestMenuComponents(tab.id, tab.reloadKey ?? 0, (error) => {
      logToNative(`[StandaloneGuestApp] Guest menu failed: ${String(error)}`);
    });
    if (v1?.ok && !menus && guestMenuNames.some((name) => v1.manifest.components.includes(name))) {
      setError('Native menus are unavailable in this Host build.');
      return;
    }
    const authorizer = authorizerRef.current;
    if (!authorizer) {
      setError('The Native capability authorization snapshot is unavailable.');
      return;
    }

    let disposed = false;
    const callLegacyActor: ActorCall = (request) => {
      const gatewayUrl =
        globalThis.__ACTRIUM_ACTR_GATEWAY_URL__ ??
        'http://127.0.0.1:8391/v1/call';
      logToNative(`[StandaloneGuestApp] legacy actor call ${request.route}`);
      return callHttpService(gatewayUrl, request);
    };
    const callServiceActor: ActorCall = (request) => {
      // The global echo gateway is legacy-bridge only. New service calls must
      // target the tab's attached Service actor.
      if (!tab.serviceCallUrl) {
        return callNativeService(NativeModules.RealmService, tab.id, request);
      }
      logToNative(`[StandaloneGuestApp] service call ${request.route}`);
      return callHttpService(tab.serviceCallUrl, request);
    };
    const localStorage = bindNativeLocalStorage(
      NativeModules.GuestLocalStorage, tab.id, tab.localStorageDocumentId
    );
    if (v1?.ok && v1.linkage.links.some(([id]) => id === 'host:local-storage') && !localStorage) {
      setError('Local storage is unavailable for this application.');
      return;
    }

    let serviceEventSource: ServiceEventSource | undefined;
    if (hasServiceOnCapability && capabilityManifestJSON) {
      try {
        const manifest = JSON.parse(capabilityManifestJSON) as CapabilityManifestV1;
        const events = serviceEventsFromManifest(manifest);
        if (events.length > 0) {
          serviceEventSource = tab.serviceEventsUrl
            ? createHttpServiceEventSource(tab.serviceEventsUrl, events)
            : createNativeServiceEventSource(
                NativeModules.RealmService,
                tab.id,
                events
              );
        }
      } catch {
        // Verification above has already surfaced the malformed manifest; keep
        // the source undefined so the engine can still load non-event guests.
      }
    }

    // Track complete Host cleanups, including revocation listeners, so engine
    // reload/tab close works even if the Guest forgets to unsubscribe.
    const eventSubscriptions = new Set<() => void>();
    const trackServiceEventSubscription = (cleanup: () => void): (() => void) => {
      if (disposed) {
        cleanup();
        return () => {};
      }
      eventSubscriptions.add(cleanup);
      return () => {
        eventSubscriptions.delete(cleanup);
      };
    };

    const fileImport = NativeModules.GuestFileImport as FileImportBridge | undefined;
    const fileExport = NativeModules.GuestFileExport as FileExportBridge | undefined;
    const nativeSave = fileExport?.save?.bind(fileExport, tab.id, tab.reloadKey ?? 0);
    const nativePick = fileImport?.pick?.bind(fileImport, tab.id, tab.reloadKey ?? 0);
    const nativePickCached = fileImport?.pickCached?.bind(fileImport, tab.id, tab.reloadKey ?? 0);
    if (v1?.ok && v1.linkage.links.some(([id]) => id === 'host:file-import') && !nativePick) {
      setError('File import is unavailable for this application.');
      return;
    }
    const speechAvailable = isGuestSpeechAvailable();
    if (v1?.ok && v1.linkage.links.some(([id]) => id === 'host:speech') && !speechAvailable) {
      setError('Speech recognition is unavailable for this application.');
      return;
    }
    const ttsBridge = NativeModules.GuestTTS as GuestTTSBridge | undefined;
    const ttsLinked = v1?.ok && v1.linkage.links.some(([id]) => id === 'host:tts');
    const reportTTSCleanupError = () => {
      logToNative('[StandaloneGuestApp] Could not stop text-to-speech playback.');
    };
    const tts = ttsLinked && typeof ttsBridge?.speak === 'function' && typeof ttsBridge.stop === 'function'
      ? new GuestTTS({
          bridge: ttsBridge,
          authorize: (capability) => authorizer.authorize(capability),
          onRevoked: (capability, listener) => authorizer.onRevoked(capability, listener),
          onCleanupError: reportTTSCleanupError,
        })
      : undefined;
    if (ttsLinked && !tts) {
      setError('Text-to-speech is unavailable for this application.');
      return;
    }
    // The fixed upload transport is native-only through the per-tab
    // RealmService module. HTTP tabs and the legacy echo tab never reach the
    // native uploader; when the manifest links uploadAttachment there, the
    // app fails closed instead of silently uploading.
    const nativeUploadSupported = !tab.serviceCallUrl && !legacyEchoTab;
    const streamBridge = nativeUploadSupported ? NativeModules.RealmService : undefined;
    const serviceStreams = streamBridge?.subscribeServiceStream && streamBridge?.unsubscribeServiceEvents
      ? new NativeServiceStreams(tab.id, streamBridge, (handler) => {
          const listener = new NativeEventEmitter(streamBridge).addListener('realmServiceEvent', handler);
          return () => listener.remove();
        })
      : undefined;
    const attachmentBridge = nativeUploadSupported
      ? (NativeModules.RealmService as RealmServiceAttachmentBridge | undefined)
      : undefined;
    const attachmentUploader = new AttachmentUploadManager({
      adapter: createRealmServiceAttachmentAdapter({
        bridge: attachmentBridge,
        emitter: DeviceEventEmitter,
        tabId: tab.id,
      }),
    });
    const attachmentsLinked =
      v1?.ok === true &&
      v1.linkage.links.some(
        ([id, exportName]) => id === 'host:service' && exportName === 'uploadAttachment'
      );
    let stopEngine: (() => void) | undefined;
    const start = async () => {
      if (attachmentsLinked) {
        // Process-wide module presence is not tab availability: a locally
        // bundled Guest shares this renderer process without owning a
        // Realm Service binding. Query the Host about this tab before the
        // engine starts so the Guest never links an unusable transport.
        const uploadAvailable = await attachmentUploader.checkAvailable();
        if (disposed) {
          attachmentUploader.dispose();
          return;
        }
        if (!uploadAvailable) {
          attachmentUploader.dispose();
          setError('Attachment upload is unavailable for this application.');
          return;
        }
      }
      if (disposed) {
        attachmentUploader.dispose();
        return;
      }
    const hostModules = createHostModules({
      hasServiceCapability,
      hasServiceOnCapability,
      callActor: callServiceActor,
      serviceEventSource,
      serviceStreams,
      sendServiceStreamChunk: streamBridge?.sendServiceStreamChunk ? async (chunk) => {
        if (disposed) throw new Error('The Service DataStream sender has ended.');
        await streamBridge.sendServiceStreamChunk(tab.id, chunk.route, chunk.streamId, chunk.sequence, chunk.payloadBase64);
      } : undefined,
      authorizeCapability: (capability) => authorizer.authorize(capability),
      onCapabilityRevoked: (capability, listener) =>
        authorizer.onRevoked(capability, listener),
      trackServiceEventSubscription,
      writeClipboardText,
      setTabTitle: (title) => setTabTitle(tab.id, tab.reloadKey ?? 0, title),
      localStorage,
      saveFile: nativeSave && ((input: FileExportSaveInput) => {
        if (disposed) throw new Error('The file export session has ended.');
        return nativeSave(input.name, input.base64);
      }),
      pickFiles: nativePick && ((input) => {
        const pick = input.cache ? nativePickCached : nativePick;
        if (!pick) throw new Error('Cached file selection is unavailable.');
        return pick(input.source ?? 'files', input.accept ?? [], input.multiple ?? false);
      }),
      readCachedFile: async (fileId) => {
        if (disposed || !fileImport?.cachedAttachment) throw new Error('The attachment cache is unavailable.');
        return fileImport.cachedAttachment(tab.id, tab.reloadKey ?? 0, 'read', fileId);
      },
      openFilePreview: async (fileId) => {
        if (disposed || !fileImport?.cachedAttachment) throw new Error('The attachment preview is unavailable.');
        const uri = await fileImport.cachedAttachment(tab.id, tab.reloadKey ?? 0, 'preview', fileId);
        if (typeof uri !== 'string' || !uri.startsWith('file:///')) throw new Error('Invalid native attachment preview.');
        if (!disposed) setPreview({ fileId, uri });
      },
      releaseCachedFile: async (fileId) => {
        if (disposed || !fileImport?.cachedAttachment) throw new Error('The attachment cache is unavailable.');
        await fileImport.cachedAttachment(tab.id, tab.reloadKey ?? 0, 'release', fileId);
        if (!disposed) setPreview((current) => current?.fileId === fileId ? null : current);
      },
      recognizeSpeech: speechAvailable ? (input, onInterim, onState) => {
        if (disposed) throw new Error('The speech recognition session has ended.');
        return recognizeSpeech(input, onInterim, onState);
      } : undefined,
      stopSpeech: speechAvailable ? async () => {
        if (!disposed) await stopSpeech();
      } : undefined,
      tts,
      attachmentUploader,
    });
    // Expose exactly the verified method and component set to this Guest Engine.
    const linked = v1?.ok ? v1 : undefined;
    const engine = linked
      ? new Engine({
          debug: false,
          ...bindV1HostModules(linked.linkage, hostModules, (capability) =>
            authorizer.authorize(capability)
          ),
          requireWhitelist: linked.manifest.imports.filter((id) => !id.startsWith('host:')),
          restrictToRegisteredComponents: true,
          // Implicit text needs a renderer even when Text is absent from the manifest.
          textNodeRenderer: Text,
        })
      : new Engine({ debug: false, contract, hostModules });
    engine.register(
      linked
        ? Object.fromEntries(
            Object.entries({ ...hostComponents, ...menus }).filter(([name]) =>
              linked.manifest.components.includes(name === 'Image' ? 'ImageAsset' : name)
            )
          )
        : hostComponents
    );
    engine.createReceiver();

    const update = () => {
      if (disposed) return;
      const next = engine.getReceiver()?.render();
      setContent(next as React.ReactElement | null);
    };
    const unsubscribeUpdate = engine.on('update', update);
    // The legacy bridge is confined to the host-designated demo package. The
    // designation comes from launch configuration, never from package data.
    // Every other package must use the per-tab host:service path.
    const unsubscribeEvents = legacyEchoTab
      ? EventHandler.setup(engine, {
      tabId: tab.id,
      onLog: (direction, event) => {
        logToNative(`[StandaloneGuestApp] ${direction}: ${event}`);
      },
      handlers: {
        ACTR_CALL: {
          responseEvent: 'ACTR_CALL_RESULT',
          handle: async (payload: {
            requestId?: string;
            routeKey?: unknown;
            payloadBase64?: unknown;
            timeoutMs?: unknown;
          }) => {
            const requestId = payload?.requestId;
            if (!hasServiceCapability) {
              return {
                requestId,
                success: false,
                error:
                  'The application does not declare the service capability.',
              };
            }
            if (
              typeof payload?.routeKey !== 'string' ||
              typeof payload?.payloadBase64 !== 'string'
            ) {
              return {
                requestId,
                success: false,
                error: 'ACTR_CALL requires routeKey and payloadBase64 strings.',
              };
            }

            logToNative(`[StandaloneGuestApp] ACTR_CALL ${payload.routeKey}`);
            const response = await callLegacyActor({
              requestId: requestId ?? `actr-${Date.now()}`,
              route: payload.routeKey,
              payloadBase64: payload.payloadBase64,
              timeoutMs:
                typeof payload.timeoutMs === 'number'
                  ? payload.timeoutMs
                  : 30_000,
            });
            logToNative(
              `[StandaloneGuestApp] ACTR_CALL completed: ${response.success ? 'success' : 'failure'}`,
            );
            return {
              requestId,
              success: response.success,
              payloadBase64: response.payloadBase64,
              error: response.message,
            };
          },
        },
      },
    })
      : () => {};

    void engine
      .loadBundle(guestScript, { containerHeight, containerWidth })
      .then(() => {
        const stats = engine.getReceiver()?.getStats();
        logToNative(
          `[StandaloneGuestApp] guest loaded: roots=${stats?.rootChildrenCount ?? 0}, nodes=${stats?.nodeCount ?? 0}`,
        );
        update();
        setTimeout(update, 0);
      })
      .catch((cause) => {
        if (!disposed) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      });

      stopEngine = () => {
        unsubscribeEvents();
        unsubscribeUpdate();
        engine.destroy();
      };
    };
    void start();

    return () => {
      disposed = true;
      cancelSpeech();
      void tts?.dispose().catch(reportTTSCleanupError);
      for (const cleanup of [...eventSubscriptions]) cleanup();
      eventSubscriptions.clear();
      attachmentUploader.dispose();
      serviceStreams?.dispose();
      stopEngine?.();
    };
  }, [
    // Do not recreate the engine on window resize. The first layout size is
    // enough for the guest bundle, and RN handles later size changes.
    // reloadKey still forces a clean reload when the package changes.
    guestScript,
    hasServiceCapability,
    hasServiceOnCapability,
    tab.id,
    tab.legacyActorBridge,
    tab.serviceCallUrl,
    tab.serviceEventsUrl,
    tab.reloadKey,
    tab.capabilityManifestJSON,
    tab.localStorageDocumentId,
    permissionsKey,
    capabilitySnapshotAvailable,
    recognizeSpeech,
    stopSpeech,
    cancelSpeech,
  ]);

  if (error) {
    return (
      <NativeView
        style={{ flex: 1, padding: 24, backgroundColor: '#2b0b0b' }}
      >
        <NativeText style={{ color: '#fda4af' }}>{error}</NativeText>
      </NativeView>
    );
  }

  const body = (
    <>
      {speechActive && (
        <NativeView style={{ flexDirection: 'row', alignItems: 'center', gap: 12, padding: 8 }}>
          <NativeText accessibilityLiveRegion='polite'>Speech recognition in progress</NativeText>
          <GuestButton title='Cancel' onPress={cancelSpeech} />
        </NativeView>
      )}
      {content ?? (
        <NativeView
          style={{
            flex: 1,
            alignItems: 'center',
            justifyContent: 'center',
            backgroundColor: '#101827',
          }}
        >
          <NativeActivityIndicator color='#60a5fa' />
        </NativeView>
      )}
      {preview ? <CachedImagePreview key={preview.uri} uri={preview.uri} close={() => setPreview(null)} /> : null}
    </>
  );
  return Supramark ? (
    <DocumentSelectionSurface key={tab.id} tabId={tab.id} writeText={setClipboardText}>
      {body}
    </DocumentSelectionSurface>
  ) : <NativeView collapsable={false} style={{ flex: 1 }}>{body}</NativeView>;
};
