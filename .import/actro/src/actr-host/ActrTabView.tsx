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
} from 'react-native';

import { logger } from '../logger';
import { installConsoleInterceptor } from '../devtools-console';
import { Engine } from 'rill';

// Install console interceptor to forward logs to DevTools Console panel
installConsoleInterceptor();
import { hostComponents } from './BundleView';
import type { ContractViolation } from 'askit/contracts';
import { createHostToGuestSender } from 'askit/contracts';
import { createEngineAdapter } from 'askit/core';
import type { ActrProps, TabConfig } from './types';
import {
  DevToolsBridge,
  logDevToolsMessage,
  logDevToolsRenderOp,
  updateDevToolsComponentTree,
} from './libs/devTools';
import { EventHandler } from './libs/eventHandler';
import { LAYOUT } from './constants';
import { evaluateTabGate, extractPanelsByPanelId } from './utils';
import Main from './views/index';
import { TabProvider } from './contexts/TabContext';
import { useAppStore } from './store/useAppStore';
import { useTabIdMapStore } from './store/useTabIdMapStore';

const log = logger.child('ActrApp');
const nerror = (msg: string) => log.error(msg);

type ActrTabViewProps = {
  tab: TabConfig;
  isActive: boolean;
  isLeftPanelVisible: boolean;
  containerWidth?: number;
  onEngineChange?: (tabId: string, engine: Engine | null) => void;
  onContractViolation?: (tabId: string, violation: ContractViolation) => void;
};

const PERMISSION_MODE: 'allow' | 'warn' | 'deny' = 'warn';

const ActrTabView: React.FC<ActrTabViewProps> = ({
  tab,
  isActive,
  isLeftPanelVisible,
  containerWidth: propsContainerWidth,
  onContractViolation,
  onEngineChange,
}) => {
  const gate = evaluateTabGate(tab);

  // 🆕 通过 tab.id 获取 appId
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);

  // 🆕 通过 appId 获取 guest script
  const guestScript = useAppStore((state) =>
    appId ? state.getGuestScript(appId) : undefined,
  );
  const guestLoadingStatus = useAppStore((state) =>
    appId ? state.guestLoadingStatus[appId] : undefined,
  );

  // Engine 状态（按 tab 维度隔离）
  const [engine, setEngine] = useState<Engine | null>(null);
  const [isLoading, setIsLoading] = useState(isActive);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [leftPanel, setLeftPanel] = useState<React.ReactElement | null>(null);
  const [rightPanel, setRightPanel] = useState<React.ReactElement | null>(null);

  const engineRef = useRef<Engine | null>(null);
  const adapterRef = useRef<{ dispose: () => void } | null>(null);
  const loadedUnifiedScriptRef = useRef<string | null>(null);
  const isActiveRef = useRef(isActive);
  const pendingUpdateRef = useRef(false);
  const lastBatchesRef = useRef<number>(0);

  React.useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

  // 优化：使用固定宽度 + flexShrink，让 Yoga 自动处理收缩
  // 不再依赖 propsContainerWidth 计算，避免 re-render
  const rightWidth = useMemo(
    () => (isLeftPanelVisible ? LAYOUT.RIGHT_PANEL_WIDTH : 0),
    [isLeftPanelVisible],
  );

  useEffect(() => {
    if (guestLoadingStatus) return;
    // guest 解析完成
    useAppStore.getState().setGuestLoadingStatus(appId, true);
  }, [rightPanel])

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
        const unsubscribeMessage = EventHandler.setup(currentEngine, {
          tabId: tab.id,
          onLog: logDevToolsMessage,
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
  }, [isActive, tab.id, tab.unifiedScript]);

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
  const threeColumnStyle = styles.threeColumn;

  const main = React.useMemo(() => <Main />, []);

  if (isActive && gate.status === 'blocked') {
    return (
      <View
        style={{
          flex: 1,
          alignItems: 'center',
          justifyContent: 'center',
          padding: 16,
          backgroundColor: '#1a1a1a',
        }}
      >
        <Text
          style={{
            color: '#ff6b6b',
            fontSize: 18,
            fontWeight: '600',
            marginBottom: 12,
          }}
        >
          已阻止运行
        </Text>
        <Text style={{ color: '#ddd', fontSize: 13, textAlign: 'center' }}>
          {gate.blockedReason ?? '不满足宿主 contract/权限策略'}
        </Text>
      </View>
    );
  }

  // 加载中（仅显示当前 active 的 tab）
  if (isActive && isLoading) {
    return (
      <View
        style={{
          flex: 1,
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: '#0a0a0a',
        }}
      >
        <ActivityIndicator size='large' color='#007AFF' />
        <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>
          初始化 Single Engine (usePanels mode)...
        </Text>
      </View>
    );
  }

  // 加载错误
  if (isActive && loadError) {
    return (
      <View
        style={{
          flex: 1,
          alignItems: 'center',
          justifyContent: 'center',
          padding: 16,
          backgroundColor: '#1a1a1a',
        }}
      >
        <Text
          style={{
            color: '#ff6b6b',
            fontSize: 18,
            fontWeight: '600',
            marginBottom: 12,
          }}
        >
          Engine 初始化失败
        </Text>
        <Text
          style={{
            color: '#ccc',
            fontSize: 14,
            textAlign: 'center',
            marginBottom: 8,
          }}
        >
          {loadError.message}
        </Text>
      </View>
    );
  }

  return (
    <TabProvider tab={tab}>
      <View style={threeColumnStyle}>
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

        {/* 中间对话窗口 - flex 自适应 */}
        <View style={styles.centerColumn}>{main}</View>

        {/* 右侧扩展面板 - 基准宽度，窗口变小时允许收缩 */}
        {isLeftPanelVisible && rightPanel && (
          <View
            style={[
              styles.rightPanel,
              {
                width: rightWidth,
              },
            ]}
          >
            {rightPanel}
          </View>
        )}
      </View>
    </TabProvider>
  );
};

export default ActrTabView;

const styles = StyleSheet.create({
  threeColumn: {
    flex: 1,
    flexDirection: 'row',
    backgroundColor: '#0a0a0a',
    minWidth: 0, // 允许内部列在窗口缩小时收缩
  },
  leftPanel: {
    borderRightWidth: 1,
    borderRightColor: LAYOUT.BORDER_COLOR,
    flexShrink: 1,
    minWidth: LAYOUT.LEFT_PANEL_MIN_WIDTH, // 最小宽度，允许 Yoga 自动收缩
  },
  centerColumn: {
    flex: 1,
    backgroundColor: '#ffffff', // White background like the source
    minWidth: LAYOUT.CENTER_MIN_WIDTH,
    justifyContent: 'space-between',
  },

  rightPanel: {
    flexShrink: 1,
    minWidth: LAYOUT.RIGHT_PANEL_MIN_WIDTH, // 最小宽度，允许 Yoga 自动收缩
    position: 'absolute',
    right: 0,
    height: '100%',
  },
  container: {
    flex: 1,
    position: 'relative',
  },
  empty: {
    flex: 1,
    backgroundColor: '#111',
  },
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#1a1a2a',
  },
  loadingText: {
    color: '#888',
    fontSize: 14,
  },
  tabHost: {
    flex: 1,
    position: 'relative',
  },
});
