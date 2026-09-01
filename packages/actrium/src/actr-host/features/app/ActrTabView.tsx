import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  View,
  Text,
  ScrollView,
  TouchableOpacity,
  ActivityIndicator,
  NativeModules,
  StyleSheet,
  Pressable,
  useWindowDimensions,
} from 'react-native';
import { GestureDetector } from 'react-native-gesture-handler';
import Animated, {
  useSharedValue,
  useAnimatedStyle,
} from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTranslation } from 'react-i18next';

import { logger } from '../../../logger';
import { installConsoleInterceptor } from '../../../devtools-console';
import { Engine } from 'keel/host';

// Install console interceptor to forward logs to DevTools Console panel
installConsoleInterceptor();
import { hostComponents } from './BundleView';
import type { ContractViolation } from 'askit/contracts';
import { createHostToGuestSender } from 'askit/contracts';
import { createEngineAdapter } from 'askit/core';
import type { ActrProps, TabConfig } from '../../types';
import {
  DevToolsBridge,
  logDevToolsMessage,
  logDevToolsRenderOp,
  updateDevToolsComponentTree,
} from '../../libs/devTools';
import { EventHandler } from 'askit';
import { LAYOUT } from '../../constants';
import { evaluateTabGate, extractPanelsByPanelId } from '../../utils';
import Main from './views.index';
import { TabProvider } from '../../contexts/TabContext';
import { useAppStore } from '../../store/useAppStore';
import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import ExtensionPanel from '../extension/ExtensionPanel';
import { ChatHeader } from '../chat/ChatHeader';
import { useTabNativeEvents } from '../../hooks/useTabNativeEvents';
import { useExtensionPanelGesture } from '../../hooks/useExtensionPanelGesture';
import { isTablet, isAndroid, isPhone } from '../../../utils/device';
import { useExtensionPanelVisibility } from '../../hooks/useExtensionPanelVisibility';
import { AppText } from '../../../components';
import { exitFullscreen } from '../../../native-bridge';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { useExtensionEvent } from './useExtensionEvent';

const log = logger.child('ActrApp');
const nerror = (msg: string) => log.error(msg);

type ActrTabViewProps = {
  tab: TabConfig;
  isActive: boolean;
  isLeftPanelVisible: boolean;
  containerWidth?: number;
  containerHeight?: number;
  onEngineChange?: (tabId: string, engine: Engine | null) => void;
  onContractViolation?: (tabId: string, violation: ContractViolation) => void;
};

const PERMISSION_MODE: 'allow' | 'warn' | 'deny' = 'warn';

const ActrTabView: React.FC<ActrTabViewProps> = ({
  tab,
  isActive,
  isLeftPanelVisible,
  containerWidth: propsContainerWidth,
  containerHeight: propsContainerHeight,
  onContractViolation,
  onEngineChange,
}) => {
  const { top: topInsetRaw } = useSafeAreaInsets();
  const { t } = useTranslation();
  const gate = evaluateTabGate(tab);
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  // 🆕 Pass. tab.id Access appId
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  // 🆕 Pass. appId Access guest script
  const guestScript = useAppStore((state) =>
    appId ? state.getGuestScript(appId) : undefined,
  );
  const guestLoadingStatus = useAppStore((state) =>
    appId ? state.guestLoadingStatus[appId] : undefined,
  );
  const instanceName = useAppStore((state) =>
    appId ? state.apps[appId]?.instance_name : undefined,
  );

  // Engine Status (Pressed) tab (Dives isolated)
  const [engine, setEngine] = useState<Engine | null>(null);
  const [isLoading, setIsLoading] = useState(isActive);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [leftPanel, setLeftPanel] = useState<React.ReactElement | null>(null);
  const [rightPanel, setRightPanel] = useState<React.ReactElement | null>(null);
  const {
    panelVisible,
    handlePhonePanelVisibleChange,
    togglePhonePanelVisible,
    closePhonePanel,
  } = useExtensionPanelVisibility({
    tabId: tab.id,
    isLeftPanelVisible,
  });
  const handleExitTab = React.useCallback(async () => {
    try {
      await exitFullscreen();
    } catch (error) {
      console.error('[ActrTabView] Failed to exit fullscreen', error);
    }
  }, []);

  const engineRef = useRef<Engine | null>(null);
  const adapterRef = useRef<{ dispose: () => void } | null>(null);
  const loadedUnifiedScriptRef = useRef<string | null>(null);
  const isActiveRef = useRef(isActive);
  const pendingUpdateRef = useRef(false);
  const lastBatchesRef = useRef<number>(0);

  // Animation value of the middle window (inPhone Mode by Gesture + ExtensionPanel Driver)
  const centerAnimValue = useSharedValue(0);
  // Panel Slide Position (by hook and PhoneExtensionPanel Share)
  const slideAnim = useSharedValue(0);
  // Optimization: use of fixed width + flexShrinkJean. Yoga Automatically process contractions
  // No more relying propsContainerWidth Calculate, Avoid re-render
  // Note:rightWidth It should always be fixed, not based on isLeftPanelVisible Change
  const rightWidth = LAYOUT.RIGHT_PANEL_WIDTH;

  // Expand Panel Gesture (Peer)Phone Mode: Right Slide Open/Left slide off;Tablet Mode: empty gestures)
  const { gesture: panelGesture, isGesturing } = useExtensionPanelGesture({
    panelWidth: rightWidth,
    slideAnim,
    centerAnimValue,
    onVisibilityChange: handlePhonePanelVisibleChange,
  });

  const handlers = useExtensionEvent(tab.id, {
    closePhonePanel,
  });

  React.useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

  useEffect(() => {
    if (guestLoadingStatus) return;
    // guest Parsing complete
    useAppStore.getState().setGuestLoadingStatus(appId, true);
  }, [rightPanel]);

  // Update echo: Extract Panel
  const handleUpdate = useCallback(() => {
    if (!isActiveRef.current) {
      pendingUpdateRef.current = true;
      return;
    }
    const eng = engineRef.current;
    if (!eng) return;
    const receiver = eng.getReceiver();
    if (!receiver) return;

    const receiverStats = receiver.getStats?.();
    if (!receiverStats || receiverStats.nodeCount === 0) return;

    // ♪ Only if ♪ batches Update Panels When Change
    const currentBatches = receiverStats.totalBatches;
    if (currentBatches === lastBatchesRef.current) return;
    lastBatchesRef.current = currentBatches;

    const fullTree = receiver.render();
    const panels = extractPanelsByPanelId(fullTree);
    setLeftPanel(panels.left);
    setRightPanel(panels.right);

    // Update DevTools component tree
    updateDevToolsComponentTree(eng, tab.id);
  }, [tab.id]);

  // Cleaning up of components when offloaded (tab Close)
  React.useEffect(() => {
    return () => {
      adapterRef.current?.dispose();
      adapterRef.current = null;
      engineRef.current?.destroy();
      engineRef.current = null;
      onEngineChange?.(tab.id, null);
      loadedUnifiedScriptRef.current = null;
      setEngine(null);
    };
  }, []);

  // Listen to access tab And quit. tab Events (unreading, change of collection, etc.)
  useTabNativeEvents(tab.id);

  // Host -> Guest: Visible events (used for Guest Soft sleep./(Recovered)
  React.useEffect(() => {
    const eng = engineRef.current;
    if (!eng) return;
    const send = createHostToGuestSender(eng, {
      onViolation: (v) => onContractViolation?.(tab.id, v),
    });
    const visibilityPayload = { visible: isActive, tabId: tab.id };
    logDevToolsMessage(
      'hostToGuest',
      'HOST_VISIBILITY',
      visibilityPayload,
      tab.id,
    );
    send('HOST_VISIBILITY', visibilityPayload);
    if (isActive && pendingUpdateRef.current) {
      pendingUpdateRef.current = false;
      handleUpdate();
    }
  }, [engine, isActive, onContractViolation, tab.id, handleUpdate]);

  // Only first time in. tab(active) Initialize Engine
  React.useEffect(() => {
    if (!isActive) return;
    if (gate.status === 'blocked') return;
    if (engineRef.current) return;

    let cancelled = false;
    let currentEngine: Engine | null = null;
    let currentAdapter: { dispose: () => void } | null = null;

    async function init() {
      try {
        setIsLoading(true);
        setLoadError(null);

        currentEngine = new Engine({
          debug: false,
          logger: {
            log: (...args: unknown[]) =>
              log.info(args.map((a) => String(a)).join(' ')),
            warn: (...args: unknown[]) =>
              log.warning(args.map((a) => String(a)).join(' ')),
            error: (...args: unknown[]) =>
              nerror(args.map((a) => String(a)).join(' ')),
          },
        });
        currentEngine.register(hostComponents);

        // Generic message handler with DevTools logging
        // Registered from Guest The message processor. Everything will be recorded. DevTools
        const unsubscribeMessage = EventHandler.setup(currentEngine, {
          tabId: tab.id,
          onLog: logDevToolsMessage,
          handlers,
        });

        // Render operation handler with DevTools logging
        const unsubscribeOperation = currentEngine.on('operation', (batch) => {
          // Log each operation in the batch to DevTools
          for (const op of batch.operations) {
            // Note: Operation uses 'op' field for operation type (CREATE/UPDATE/DELETE/TEXT etc.)
            // and 'type' field for component type (View/Text etc.)
            const opType = (op as { op: string }).op;
            logDevToolsRenderOp(
              opType,
              opType === 'CREATE' ? (op as { type?: string }).type : undefined,
              String((op as { id?: number }).id),
              (op as { props?: unknown }).props,
              tab.id,
            );
          }
        });

        currentAdapter = createEngineAdapter(currentEngine, {
          onContractViolation: (v) => onContractViolation?.(tab.id, v),
          permissions: tab.permissions ?? [],
          permissionMode: PERMISSION_MODE,
        });

        // Store unsubscribe for cleanup
        const originalDispose = currentAdapter.dispose;
        currentAdapter.dispose = () => {
          unsubscribeMessage();
          unsubscribeOperation();
          originalDispose();
        };

        if (cancelled) {
          currentAdapter?.dispose();
          currentEngine?.destroy();
          return;
        }

        engineRef.current = currentEngine;
        adapterRef.current = currentAdapter;
        setEngine(currentEngine);
        onEngineChange?.(tab.id, currentEngine);
        setIsLoading(false);
      } catch (e: any) {
        if (cancelled) return;
        nerror(`[ActrTabView] init failed: ${e?.message}`);
        setLoadError(e instanceof Error ? e : new Error(String(e)));
        setIsLoading(false);
      }
    }

    init();

    return () => {
      cancelled = true;
      // It's not destroyed here. Engine(Present) tab status; only cancel incomplete init
      void currentEngine;
      void currentAdapter;
    };
  }, [isActive, tab.id, tab.unifiedScript, engine]);

  // Load unified bundle(Each) tab It's... Engine One each)
  React.useEffect(() => {
    if (!isActive) return;
    if (gate.status === 'blocked') return;
    if (!engine) return;
    if (engine.isDestroyed) {
      nerror(`[ActrTabView] Engine Destroyed, reconstructed tab=${tab.id}`);
      adapterRef.current?.dispose();
      adapterRef.current = null;
      engineRef.current = null;
      onEngineChange?.(tab.id, null);
      setEngine(null);
      setIsLoading(true);
      setLoadError(null);
      setLeftPanel(null);
      setRightPanel(null);
      return;
    }
    const eng = engine;

    // 🆕 Use Priority store Medium guestScript, and the second tab.unifiedScript(Reverse compatibility)
    const unifiedScript = guestScript;
    // const unifiedScript = guestScript || tab.unifiedScript;
    if (!unifiedScript) return;

    const script = unifiedScript;

    // If script changes, rebuild Engine
    if (
      loadedUnifiedScriptRef.current &&
      loadedUnifiedScriptRef.current !== script
    ) {
      adapterRef.current?.dispose();
      adapterRef.current = null;
      eng.destroy();
      engineRef.current = null;
      onEngineChange?.(tab.id, null);
      loadedUnifiedScriptRef.current = null;
      setEngine(null);
      setIsLoading(true);
      setLoadError(null);
      setLeftPanel(null);
      setRightPanel(null);
      return;
    }

    const unsubscribeUpdate = eng.on('update', handleUpdate);

    if (eng.isLoaded) return unsubscribeUpdate;

    async function loadUnifiedBundle() {
      try {
        loadedUnifiedScriptRef.current = script;
        eng.createReceiver();
        await eng.loadBundle(script, {
          containerHeight: 977,
          containerWidth: 1400,
        });

        // Make sure you get the panel.
        handleUpdate();
        setTimeout(handleUpdate, 0);
        setTimeout(handleUpdate, 50);

        // Announcements Native Load completed
        try {
          NativeModules.SnapshotModule?.onContentReady?.(tab.id);
        } catch {}
      } catch (err: any) {
        nerror(`[ActrTabView] bundle load failed: ${err?.message}`);
        setLoadError(err instanceof Error ? err : new Error(String(err)));
        if (eng.isDestroyed) {
          adapterRef.current?.dispose();
          adapterRef.current = null;
          engineRef.current = null;
          onEngineChange?.(tab.id, null);
          setEngine(null);
          setIsLoading(true);
          setLeftPanel(null);
          setRightPanel(null);
        }
      }
    }

    loadUnifiedBundle();
    return unsubscribeUpdate;
  }, [engine, isActive, tab.id, guestScript, tab.unifiedScript, handleUpdate]);

  // Rendering Three-Bar Layouts - Use pure flex Layout, Jean. Yoga AutoAdaptation
  // Not dependent propsContainerWidthAvoid re-render
  // Pad The host page needs to be displayed at the top and no longer maintained Phone The simulator angle of the scene.
  const threeColumnStyle = [
    styles.threeColumn,
    isAndroid && isPhone && styles.threeColumnAndroidPhone,
    isTablet && styles.threeColumnPad,
  ];
  const sheetTopOffset = topInsetRaw;

  const main = React.useMemo(
    () => <Main />,
    [propsContainerWidth, propsContainerHeight],
  );

  // Animation style for the intermediate dialogue area (following) centerAnimValue Offset)
  const centerAnimStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: centerAnimValue.value }],
  }));

  if (isActive && gate.status === 'blocked') {
    return (
      <View style={styles.stateContainer}>
        <AppText style={styles.errorText}>Stopped running</AppText>
        <Text style={styles.errorDetail}>
          {gate.blockedReason ?? 'Not enough for the host. contract/Permission Policy'}
        </Text>
      </View>
    );
  }

  if (isActive && isLoading) {
    return (
      <View style={styles.loadingContainer}>
        <ActivityIndicator size="large" color={colors.brandPrimary} />
        <AppText style={styles.loadingText}>
          Initialize Single Engine (usePanels mode)...
        </AppText>
      </View>
    );
  }

  if (isActive && loadError) {
    return (
      <View style={styles.stateContainer}>
        <AppText style={styles.errorText}>Engine Initialization failed</AppText>
        <Text style={styles.errorDetail}>{loadError.message}</Text>
      </View>
    );
  }

  const tabContent = (
    <View style={styles.screen}>
      <View style={[threeColumnStyle, { marginTop: sheetTopOffset }]}>
        {/* Left Panel - Base width, window hour allows contraction */}
        {/* {isLeftPanelVisible && (
          <View
            style={[
              styles.leftPanel,
              {
                width: leftWidth,
              },
            ]}
          >
            {leftPanel}
          </View>
        )} */}

        {/* Header Absolute location. - Not centerColumn Animation Impact */}
        {!isTablet && (
          <ChatHeader
            onLeftPress={togglePhonePanelVisible}
            onRightPress={handleExitTab}
            appId={appId}
            leftDisabled={!appId}
            leftActive={panelVisible}
            showLeftButton={Boolean(appId)}
            instanceName={appId ? instanceName : t('actr.app.newTab')}
            includeSafeTopInset={false}
          />
        )}

        {/* Interlocutory dialogue window - flex Self-adaptation, driven by extended panels (Phone Mode) */}
        <Animated.View style={[styles.centerColumn, centerAnimStyle]}>
          {main}
          {/* Click mask for the dialogue area when the extended panel is open — Click to close the panel */}
          {panelVisible && (
            <Pressable
              style={styles.dismissOverlay}
              onPress={closePhonePanel}
            />
          )}
        </Animated.View>

        {/* Extension Panel - Tablet Fixed Right / Phone Left slide (support gesture) */}
        <ExtensionPanel
          visible={panelVisible}
          content={rightPanel}
          width={rightWidth}
          centerAnimValue={centerAnimValue}
          slideAnim={slideAnim}
          isGesturing={isGesturing}
        />
      </View>
    </View>
  );

  return (
    <TabProvider tab={tab}>
      {/* Pad No skating, skipping. GestureDetector, avoid the gestures swallowing the clicks. */}
      {isTablet ? (
        tabContent
      ) : (
        <GestureDetector gesture={panelGesture}>{tabContent}</GestureDetector>
      )}
    </TabProvider>
  );
};

export default ActrTabView;

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    threeColumn: {
      flex: 1,
      flexDirection: 'row',
      backgroundColor: colors.bgColumn,
      minWidth: 0,
    },
    threeColumnAndroidPhone: {
      overflow: 'hidden',
      borderTopLeftRadius: 38,
      borderTopRightRadius: 38,
    },
    threeColumnPad: {
      borderTopLeftRadius: 0,
      borderTopRightRadius: 0,
    },
    screen: {
      flex: 1,
      backgroundColor: colors.bgScreen,
    },
    leftPanel: {
      borderRightWidth: 1,
      borderRightColor: LAYOUT.BORDER_COLOR,
      flexShrink: 1,
      minWidth: LAYOUT.LEFT_PANEL_MIN_WIDTH,
    },
    centerColumn: {
      flex: 1,
      backgroundColor: colors.bgColumn,
      minWidth: LAYOUT.CENTER_MIN_WIDTH,
      justifyContent: 'space-between',
    },
    dismissOverlay: {
      ...StyleSheet.absoluteFillObject,
      zIndex: 10,
    },
    container: {
      flex: 1,
      position: 'relative',
    },
    empty: {
      flex: 1,
      backgroundColor: colors.bgEmpty,
    },
    stateContainer: {
      flex: 1,
      alignItems: 'center',
      justifyContent: 'center',
      padding: 16,
      backgroundColor: colors.bgBlocked,
    },
    loadingContainer: {
      flex: 1,
      alignItems: 'center',
      justifyContent: 'center',
      backgroundColor: colors.bgLoading,
    },
    errorText: {
      color: colors.textError,
      ...getTypographyStyle('statusTitle'),
      marginBottom: spacing.md,
    },
    errorDetail: {
      color: colors.textSecondary,
      fontSize: 13,
      textAlign: 'center',
    },
    loadingText: {
      color: colors.textLoading,
      ...getTypographyStyle('body'),
      marginTop: spacing.lg,
    },
    tabHost: {
      flex: 1,
      position: 'relative',
    },
  });
