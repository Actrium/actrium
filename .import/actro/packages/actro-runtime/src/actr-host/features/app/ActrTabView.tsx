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
import { Engine } from 'rill';

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

  // 🆕 通过 tab.id 获取 appId
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  // 🆕 通过 appId 获取 guest script
  const guestScript = useAppStore((state) =>
    appId ? state.getGuestScript(appId) : undefined,
  );
  const guestLoadingStatus = useAppStore((state) =>
    appId ? state.guestLoadingStatus[appId] : undefined,
  );
  const instanceName = useAppStore((state) =>
    appId ? state.apps[appId]?.instance_name : undefined,
  );

  // Engine 状态（按 tab 维度隔离）
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

  // 中间窗口的动画值（Phone 模式由手势 + ExtensionPanel 驱动）
  const centerAnimValue = useSharedValue(0);
  // 面板滑动位置（由 hook 和 PhoneExtensionPanel 共享）
  const slideAnim = useSharedValue(0);
  // 优化：使用固定宽度 + flexShrink，让 Yoga 自动处理收缩
  // 不再依赖 propsContainerWidth 计算，避免 re-render
  // 注意：rightWidth 应该始终是固定值，不要根据 isLeftPanelVisible 变化
  const rightWidth = LAYOUT.RIGHT_PANEL_WIDTH;

  // 扩展面板手势（Phone 模式：右滑打开/左滑关闭；Tablet 模式：空手势）
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
    // guest 解析完成
    useAppStore.getState().setGuestLoadingStatus(appId, true);
  }, [rightPanel]);

  // 更新回调：提取面板
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

    // 只有当 batches 变化时才更新面板
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

  // 组件卸载时清理（tab 关闭）
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

  // 监听进入 tab 和退出 tab 事件（未读清除、收藏变化等）
  useTabNativeEvents(tab.id);

  // Host -> Guest：可见性事件（用于 Guest 软睡眠/恢复）
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

  // 仅在首次进入该 tab（active）时初始化 Engine
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
        // 注册来自 Guest 的消息处理器，所有消息都会被记录到 DevTools
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
      // 这里不销毁 Engine（保持 tab 状态）；仅取消未完成的 init
      void currentEngine;
      void currentAdapter;
    };
  }, [isActive, tab.id, tab.unifiedScript, engine]);

  // 加载 unified bundle（每个 tab 的 Engine 各加载一次）
  React.useEffect(() => {
    if (!isActive) return;
    if (gate.status === 'blocked') return;
    if (!engine) return;
    if (engine.isDestroyed) {
      nerror(`[ActrTabView] Engine 已销毁，重建 tab=${tab.id}`);
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

    // 🆕 优先使用 store 中的 guestScript，其次是 tab.unifiedScript（向后兼容）
    const unifiedScript = guestScript;
    // const unifiedScript = guestScript || tab.unifiedScript;
    if (!unifiedScript) return;

    const script = unifiedScript;

    // 如果脚本变化，重建 Engine
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

    if (eng.isLoaded) return;

    async function loadUnifiedBundle() {
      try {
        loadedUnifiedScriptRef.current = script;
        eng.createReceiver(handleUpdate);
        await eng.loadBundle(script, {
          containerHeight: 977,
          containerWidth: 1400,
        });

        // 确保面板提取
        handleUpdate();
        setTimeout(handleUpdate, 0);
        setTimeout(handleUpdate, 50);

        // 通知 Native 内容已加载完成
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
  }, [engine, isActive, tab.id, guestScript, tab.unifiedScript, handleUpdate]);

  // 渲染三栏布局 - 使用纯 flex 布局，让 Yoga 自动适应
  // 不依赖 propsContainerWidth，避免 re-render
  // Pad 宿主页需要贴齐顶部展示，不再保留 Phone 场景的模拟圆角。
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

  // 中间对话区域的动画样式（跟随 centerAnimValue 偏移）
  const centerAnimStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: centerAnimValue.value }],
  }));

  if (isActive && gate.status === 'blocked') {
    return (
      <View style={styles.stateContainer}>
        <AppText style={styles.errorText}>已阻止运行</AppText>
        <Text style={styles.errorDetail}>
          {gate.blockedReason ?? '不满足宿主 contract/权限策略'}
        </Text>
      </View>
    );
  }

  if (isActive && isLoading) {
    return (
      <View style={styles.loadingContainer}>
        <ActivityIndicator size="large" color={colors.brandPrimary} />
        <AppText style={styles.loadingText}>
          初始化 Single Engine (usePanels mode)...
        </AppText>
      </View>
    );
  }

  if (isActive && loadError) {
    return (
      <View style={styles.stateContainer}>
        <AppText style={styles.errorText}>Engine 初始化失败</AppText>
        <Text style={styles.errorDetail}>{loadError.message}</Text>
      </View>
    );
  }

  const tabContent = (
    <View style={styles.screen}>
      <View style={[threeColumnStyle, { marginTop: sheetTopOffset }]}>
        {/* 左侧面板 - 基准宽度，窗口变小时允许收缩 */}
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

        {/* Header 绝对定位 - 不受 centerColumn 动画影响 */}
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

        {/* 中间对话窗口 - flex 自适应，被扩展面板推动（Phone 模式） */}
        <Animated.View style={[styles.centerColumn, centerAnimStyle]}>
          {main}
          {/* 扩展面板打开时，对话区域的点击遮罩 — 点击关闭面板 */}
          {panelVisible && (
            <Pressable
              style={styles.dismissOverlay}
              onPress={closePhonePanel}
            />
          )}
        </Animated.View>

        {/* 扩展面板 - Tablet 右侧固定 / Phone 左侧滑入（支持手势） */}
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
      {/* Pad 不需要侧滑手势，直接跳过 GestureDetector，避免手势层吞掉点击。 */}
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
