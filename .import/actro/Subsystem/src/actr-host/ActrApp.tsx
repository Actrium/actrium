/**
 * ActrApp v8 - Single Engine Architecture with usePanels Hook Pattern
 * 单 Engine 架构 + usePanels hook 模式
 *
 * 改进：
 * - ✅ 单个 Engine 加载统一 bundle
 * - ✅ Guest 导出 usePanels hook，AUTO_RENDER_FOOTER 调用并渲染
 * - ✅ Host 通过 __panelId 属性识别面板
 * - ✅ Host 控制布局（三栏固定-flex-固定）
 * - ✅ 与 CounterMacOS 模式一致
 *
 * @version 8.0.0 - Single Engine + usePanels Hook Pattern
 */
import React, { useCallback, useRef, useState, useEffect, useMemo } from 'react';
import { View, Text, ScrollView, TouchableOpacity, ActivityIndicator, NativeModules, NativeEventEmitter, StyleSheet, findNodeHandle, UIManager } from 'react-native';
import { logger } from '../logger';
import { Engine } from 'rill';
import { hostComponents } from './BundleView';
import { EngineMonitorOverlay } from 'askit';
import type { ContractViolation } from 'askit/contracts';
import { createContractViolationCollector, createHostToGuestSender } from 'askit/contracts';
import { createEngineAdapter } from 'askit/core';
import type { ActrProps, TabConfig } from './types';

/**
 * Extract panels from React element tree by __panelId attribute
 * Searches for nodes with __panelId="left" and __panelId="right"
 */
function extractPanelsByPanelId(
  element: React.ReactNode
): { left: React.ReactElement | null; right: React.ReactElement | null } {
  if (!element || !React.isValidElement(element)) {
    return { left: null, right: null };
  }

  const result: { left: React.ReactElement | null; right: React.ReactElement | null } = {
    left: null,
    right: null,
  };

  function traverse(node: React.ReactNode): void {
    if (!React.isValidElement(node)) return;

    const props = node.props as Record<string, unknown>;
    const panelId = props.__panelId;

    if (panelId === 'left') {
      result.left = props.children as React.ReactElement;
      return;
    }
    if (panelId === 'right') {
      result.right = props.children as React.ReactElement;
      return;
    }

    const children = props.children;
    if (Array.isArray(children)) {
      children.forEach(traverse);
    } else if (children) {
      traverse(children as React.ReactNode);
    }
  }

  traverse(element);
  return result;
}

const log = logger.child('ActrApp');
const nerror = (msg: string) => log.error(msg);

// DevTools message logging helper
const DevToolsBridge = NativeModules.DevToolsBridge as {
  logMessage?: (direction: string, event: string, payload: string | null, tabId: string | null) => void;
  logRenderOp?: (operationType: string, componentType: string | null, nodeId: string | null, payload: string | null, tabId: string | null) => void;
  setEnginePaused?: (isPaused: boolean) => void;
  registerPauseHandler?: () => void;
  updateComponentTree?: (treeJson: string | null, tabId: string | null) => void;
  setInspectMode?: (enabled: boolean) => void;
  highlightNode?: (nodeId: string | null, frame: { x: number; y: number; width: number; height: number } | null) => void;
  clearHighlight?: () => void;
  selectNodeFromInspect?: (nodeId: string) => void;
  addListener?: (eventName: string) => void;
  removeListeners?: (count: number) => void;
} | undefined;

function logDevToolsMessage(
  direction: 'guestToHost' | 'hostToGuest',
  event: string,
  payload: unknown,
  tabId?: string
) {
  try {
    DevToolsBridge?.logMessage?.(
      direction,
      event,
      payload ? JSON.stringify(payload) : null,
      tabId ?? null
    );
  } catch {
    // Ignore DevTools logging errors
  }
}

function logDevToolsRenderOp(
  operationType: string,
  componentType: string | undefined,
  nodeId: string | undefined,
  props: unknown,
  tabId?: string
) {
  try {
    DevToolsBridge?.logRenderOp?.(
      operationType,
      componentType ?? null,
      nodeId ?? null,
      props ? JSON.stringify(props) : null,
      tabId ?? null
    );
  } catch {
    // Ignore DevTools logging errors
  }
}

function updateDevToolsComponentTree(engine: Engine | null, tabId?: string) {
  try {
    if (!engine) {
      DevToolsBridge?.updateComponentTree?.(null, tabId ?? null);
      return;
    }

    const receiver = engine.getReceiver();
    if (!receiver) {
      DevToolsBridge?.updateComponentTree?.(null, tabId ?? null);
      return;
    }

    // Get the component tree from Receiver
    const tree = receiver.getComponentTree?.();
    if (!tree) {
      DevToolsBridge?.updateComponentTree?.(null, tabId ?? null);
      return;
    }

    // Serialize and send to DevTools
    const treeJson = JSON.stringify(tree);
    DevToolsBridge?.updateComponentTree?.(treeJson, tabId ?? null);
  } catch {
    // Ignore DevTools errors
  }
}

// 模块级别的 props 缓存
let cachedValidProps: ActrProps | null = null;

// 布局常量
const LAYOUT = {
  LEFT_PANEL_WIDTH: 240,
  RIGHT_PANEL_WIDTH: 400,
  LEFT_PANEL_MIN_WIDTH: 140,
  RIGHT_PANEL_MIN_WIDTH: 180,
  CENTER_MIN_WIDTH: 200,
  BORDER_COLOR: '#333',
} as const;

type ActrTabViewProps = {
  tab: TabConfig;
  isActive: boolean;
  isLeftPanelVisible: boolean;
  containerWidth?: number;
  onEngineChange?: (tabId: string, engine: Engine | null) => void;
  onContractViolation?: (tabId: string, violation: ContractViolation) => void;
};

const SUPPORTED_CONTRACT = { name: 'ask', version: 1 } as const;
const PERMISSION_MODE: 'allow' | 'warn' | 'deny' = 'warn';

function evaluateTabGate(tab: TabConfig): {
  status: 'ok' | 'compat' | 'blocked';
  blockedReason?: string | null;
} {
  if (!tab.contract) return { status: 'compat', blockedReason: null };
  if (tab.contract.name !== SUPPORTED_CONTRACT.name) {
    return {
      status: 'blocked',
      blockedReason: `不支持的 contract：${tab.contract.name}（仅支持 ${SUPPORTED_CONTRACT.name}@v${SUPPORTED_CONTRACT.version}）`,
    };
  }
  if (tab.contract.version > SUPPORTED_CONTRACT.version) {
    return {
      status: 'blocked',
      blockedReason: `contractVersion 太新：${tab.contract.name}@v${tab.contract.version}（宿主仅支持到 v${SUPPORTED_CONTRACT.version}）`,
    };
  }
  return { status: 'ok', blockedReason: null };
}

const ActrTabView: React.FC<ActrTabViewProps> = ({
  tab,
  isActive,
  isLeftPanelVisible,
  containerWidth: propsContainerWidth,
  onContractViolation,
  onEngineChange,
}) => {
  const gate = evaluateTabGate(tab);

  // 右侧面板可见性（按 tab 维度保存）
  const [isRightPanelVisible, setIsRightPanelVisible] = useState(tab.rightPanelDefaultVisible ?? false);

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

  const showRightPanel = isRightPanelVisible && rightPanel;

  // 优化：使用固定宽度 + flexShrink，让 Yoga 自动处理收缩
  // 不再依赖 propsContainerWidth 计算，避免 re-render
  const leftWidth = isLeftPanelVisible ? LAYOUT.LEFT_PANEL_WIDTH : 0;
  const rightWidth = showRightPanel ? LAYOUT.RIGHT_PANEL_WIDTH : 0;

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
    logDevToolsMessage('hostToGuest', 'HOST_VISIBILITY', visibilityPayload, tab.id);
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
            log: () => {},
            warn: (...args: unknown[]) => log.warning(args.map(a => String(a)).join(' ')),
            error: (...args: unknown[]) => nerror(args.map(a => String(a)).join(' ')),
          },
        });
        currentEngine.register(hostComponents);

        // Generic message handler with DevTools logging
        const unsubscribeMessage = currentEngine.on('message', async (msg: { event: string; payload?: unknown }) => {
          // Log all Guest → Host messages to DevTools
          logDevToolsMessage('guestToHost', msg.event, msg.payload, tab.id);

          // Handle APP_FETCH_REQUEST
          if (msg.event !== 'APP_FETCH_REQUEST') return;
          const { requestId, url, options } = msg.payload as {
            requestId: string;
            url: string;
            options?: RequestInit;
          };
          try {
            const response = await fetch(url, options);
            const contentType = response.headers.get('content-type') || '';
            const data = contentType.includes('application/json')
              ? await response.json()
              : await response.text();
            const responsePayload = {
              requestId,
              ok: response.ok,
              status: response.status,
              data,
            };
            logDevToolsMessage('hostToGuest', 'APP_FETCH_RESPONSE', responsePayload, tab.id);
            currentEngine!.sendEvent('APP_FETCH_RESPONSE', responsePayload);
          } catch (error: unknown) {
            const errorPayload = {
              requestId,
              ok: false,
              status: 0,
              error: error instanceof Error ? error.message : String(error),
            };
            logDevToolsMessage('hostToGuest', 'APP_FETCH_RESPONSE', errorPayload, tab.id);
            currentEngine!.sendEvent('APP_FETCH_RESPONSE', errorPayload);
          }
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
              tab.id
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

    const unifiedScript = tab.unifiedScript;
    if (!unifiedScript) return;

    const script = unifiedScript;

    // 如果脚本变化，重建 Engine
    if (loadedUnifiedScriptRef.current && loadedUnifiedScriptRef.current !== script) {
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
  }, [engine, isActive, tab.id, tab.unifiedScript, handleUpdate]);

  // 渲染三栏布局 - 使用纯 flex 布局，让 Yoga 自动适应
  // 不依赖 propsContainerWidth，避免 re-render
  const threeColumnStyle = styles.threeColumn;

  if (isActive && gate.status === 'blocked') {
    return (
      <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
        <Text style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
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
      <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#0a0a0a' }}>
        <ActivityIndicator size="large" color="#007AFF" />
        <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>初始化 Single Engine (usePanels mode)...</Text>
      </View>
    );
  }

  // 加载错误
  if (isActive && loadError) {
    return (
      <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
        <Text style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
          Engine 初始化失败
        </Text>
        <Text style={{ color: '#ccc', fontSize: 14, textAlign: 'center', marginBottom: 8 }}>{loadError.message}</Text>
      </View>
    );
  }

  return (
    <View style={threeColumnStyle}>
      {/* 左侧面板 - 基准宽度，窗口变小时允许收缩 */}
      {isLeftPanelVisible && (
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
      )}

      {/* 中间对话窗口 - flex 自适应 */}
      <View style={styles.centerColumn}>
        <ScrollView
          style={{ flex: 1 }}
          contentContainerStyle={{ padding: 20 }}
          showsHorizontalScrollIndicator={false}
          showsVerticalScrollIndicator={false}
        >
          <Text style={{ color: '#fff', fontSize: 24, marginBottom: 20, fontWeight: 'bold' }}>
            Canvas ActrApp v8
          </Text>

          {/* 右侧面板控制按钮 */}
          <TouchableOpacity
            onPress={() => setIsRightPanelVisible(!isRightPanelVisible)}
            style={{
              padding: 12,
              backgroundColor: isRightPanelVisible ? '#ff6b6b' : '#4CAF50',
              borderRadius: 6,
              marginBottom: 20,
              alignItems: 'center',
            }}
          >
            <Text style={{ color: '#fff', fontSize: 14, fontWeight: 'bold' }}>
              {isRightPanelVisible ? '◀ 隐藏右侧面板' : '▶ 显示右侧面板'}
            </Text>
          </TouchableOpacity>

          {/* Tab 信息 */}
          <View style={{ marginBottom: 16, padding: 12, backgroundColor: '#1a1a1a', borderRadius: 8 }}>
            <Text style={{ color: '#00ff00', fontSize: 16, fontWeight: 'bold', marginBottom: 8 }}>
              ✅ 当前 Tab
            </Text>
            <Text style={{ color: '#fff', fontSize: 14, marginBottom: 4 }}>
              标题: {tab.title || '未命名'}
            </Text>
            <Text style={{ color: '#fff', fontSize: 14 }}>
              ID: {tab.id || 'N/A'}
            </Text>
          </View>

          {/* Contracts / Permissions（Phase 1：提示） */}
          <View style={{ marginBottom: 16, padding: 12, backgroundColor: '#101827', borderRadius: 8 }}>
            <Text style={{ color: '#93c5fd', fontSize: 14, fontWeight: 'bold', marginBottom: 8 }}>
              📜 Contracts / 🔐 Permissions
            </Text>
            <Text style={{ color: '#e5e7eb', fontSize: 12, marginBottom: 4 }}>
              contract: {tab.contract ? `${tab.contract.name}@v${tab.contract.version}` : '（未声明/兼容模式）'}
            </Text>
            <Text style={{ color: '#e5e7eb', fontSize: 12 }}>
              permissions: {(tab.permissions && tab.permissions.length > 0) ? tab.permissions.join(', ') : '（无）'}
            </Text>
            {gate.status === 'compat' && (
              <Text style={{ color: '#fbbf24', fontSize: 11, marginTop: 8 }}>
                ⚠️ 未声明 contract：将以兼容模式运行；建议补齐 `manifest.contract` 以便宿主做版本协商与诊断。
              </Text>
            )}
            {tab.permissions && tab.permissions.length > 0 && (
              <Text style={{ color: '#fbbf24', fontSize: 11, marginTop: 8 }}>
                ⚠️ 该 Guest 声明了权限；宿主可按策略 warn/deny（当前：{PERMISSION_MODE}）。
              </Text>
            )}
          </View>

          {/* v8 架构说明 */}
          <View style={{
            marginTop: 20,
            padding: 12,
            backgroundColor: '#1a2a2a',
            borderRadius: 8,
            borderLeftWidth: 4,
            borderLeftColor: '#00aaff'
          }}>
            <Text style={{ color: '#88ddff', fontSize: 12, fontWeight: 'bold', marginBottom: 8 }}>
              🚀 v8 Single Engine + usePanels Hook 架构
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11, marginBottom: 4 }}>
              ✅ 单 Engine 加载统一 bundle
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11, marginBottom: 4 }}>
              ✅ Guest 导出 usePanels hook
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11, marginBottom: 4 }}>
              ✅ __panelId 属性识别面板
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11 }}>
              ✅ 与 CounterMacOS 模式一致
            </Text>
          </View>
        </ScrollView>
      </View>

      {/* 右侧扩展面板 - 基准宽度，窗口变小时允许收缩 */}
      {showRightPanel && (
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
  );
};

export const ActrApp: React.FC<ActrProps> = (props) => {
  // Props 稳定化
  const hasValidTabs = props?.tabs && props.tabs.length > 0;
  let effectiveProps = props;

  if (hasValidTabs) {
    cachedValidProps = { ...props };
  } else if (cachedValidProps) {
    effectiveProps = cachedValidProps;
  }

  const { tabs = [], currentTabId, isLeftPanelVisible = true, containerWidth, containerHeight } = effectiveProps || {};

  if (!tabs.length) {
    return <View style={{ flex: 1, backgroundColor: '#111' }} />;
  }

  const activeTabId = currentTabId ?? tabs[0]!.id;

  const monitorTabs = React.useMemo(
    () =>
      tabs.map((t) => {
        const gate = evaluateTabGate(t);
        return { ...t, status: gate.status, blockedReason: gate.blockedReason };
      }),
    [tabs]
  );

  const engineMapRef = useRef<Map<string, Engine>>(new Map());
  const contractCollectorsRef = useRef<Map<string, ReturnType<typeof createContractViolationCollector>>>(
    new Map()
  );

  const recordContractViolation = useCallback((tabId: string, violation: ContractViolation) => {
    let collector = contractCollectorsRef.current.get(tabId);
    if (!collector) {
      collector = createContractViolationCollector({ max: 50 });
      contractCollectorsRef.current.set(tabId, collector);
    }
    collector.record(violation);
  }, []);

  const handleEngineChange = useCallback((tabId: string, next: Engine | null) => {
    if (next) {
      engineMapRef.current.set(tabId, next);
    } else {
      engineMapRef.current.delete(tabId);
      contractCollectorsRef.current.delete(tabId);
    }
  }, []);

  // Inspect mode state
  const [isInspectMode, setIsInspectMode] = useState(false);

  // Highlighted node frame for bidirectional linking
  const [highlightedFrame, setHighlightedFrame] = useState<{
    x: number;
    y: number;
    width: number;
    height: number;
    nodeId: string;
  } | null>(null);

  // Node frames cache for hit testing
  const nodeFramesRef = useRef<Map<string, { x: number; y: number; width: number; height: number }>>(new Map());

  // Measure all node frames when entering inspect mode or when tree updates
  const measureAllNodeFrames = useCallback((engine: Engine | null) => {
    console.log('[DevTools] measureAllNodeFrames called');
    if (!engine) {
      console.log('[DevTools] No engine for measureAllNodeFrames');
      return;
    }

    const receiver = engine.getReceiver();
    if (!receiver) {
      console.log('[DevTools] No receiver for measureAllNodeFrames');
      return;
    }

    const allRefs = receiver.getAllNodeRefs?.();
    if (!allRefs) {
      console.log('[DevTools] No refs from getAllNodeRefs');
      return;
    }

    console.log('[DevTools] Measuring', allRefs.size, 'nodes');
    const frames = new Map<string, { x: number; y: number; width: number; height: number }>();
    let measureCount = 0;

    allRefs.forEach((ref, nodeId) => {
      if (!ref.current) return;

      // Try using component's measure method directly
      const component = ref.current as { measure?: (callback: (x: number, y: number, width: number, height: number, pageX: number, pageY: number) => void) => void };
      if (typeof component.measure === 'function') {
        component.measure((x, y, width, height, pageX, pageY) => {
          if (width > 0 && height > 0) {
            frames.set(String(nodeId), { x: pageX, y: pageY, width, height });
            measureCount++;
          }
        });
        return;
      }

      // Fallback to UIManager.measure
      const handle = findNodeHandle(ref.current as React.Component);
      if (!handle) return;

      UIManager.measure(handle, (x, y, width, height, pageX, pageY) => {
        if (width > 0 && height > 0) {
          frames.set(String(nodeId), { x: pageX, y: pageY, width, height });
          measureCount++;
        }
      });
    });

    // Update after all measurements complete
    setTimeout(() => {
      console.log('[DevTools] Measured', measureCount, 'nodes, total frames:', frames.size);
      nodeFramesRef.current = frames;
    }, 100);
  }, []);

  // Measure single node frame and highlight it
  const measureAndHighlightNode = useCallback((nodeId: string, engine: Engine | null) => {
    console.log('[DevTools] measureAndHighlightNode called with nodeId:', nodeId);
    if (!engine) {
      console.log('[DevTools] No engine available');
      return;
    }

    const receiver = engine.getReceiver();
    if (!receiver) {
      console.log('[DevTools] No receiver available');
      return;
    }

    const ref = receiver.getNodeRef?.(Number(nodeId));
    console.log('[DevTools] Got ref for node:', nodeId, 'ref:', ref, 'current:', ref?.current);
    if (!ref?.current) {
      console.log('[DevTools] No ref.current, clearing highlight');
      setHighlightedFrame(null);
      return;
    }

    // Try using component's measure method directly (more reliable on macOS)
    const component = ref.current as { measure?: (callback: (x: number, y: number, width: number, height: number, pageX: number, pageY: number) => void) => void };
    if (typeof component.measure === 'function') {
      console.log('[DevTools] Using component.measure directly');
      component.measure((x, y, width, height, pageX, pageY) => {
        console.log('[DevTools] component.measure result:', { x, y, width, height, pageX, pageY });
        if (width > 0 && height > 0) {
          setHighlightedFrame({ x: pageX, y: pageY, width, height, nodeId });
          DevToolsBridge?.highlightNode?.(nodeId, { x: pageX, y: pageY, width, height });
        } else {
          console.log('[DevTools] Invalid dimensions, clearing highlight');
          setHighlightedFrame(null);
          DevToolsBridge?.clearHighlight?.();
        }
      });
      return;
    }

    // Fallback to UIManager.measure
    console.log('[DevTools] Fallback to UIManager.measure');
    const handle = findNodeHandle(ref.current as React.Component);
    console.log('[DevTools] findNodeHandle result:', handle);
    if (!handle) {
      console.log('[DevTools] No handle, clearing highlight');
      setHighlightedFrame(null);
      return;
    }

    UIManager.measure(handle, (x, y, width, height, pageX, pageY) => {
      console.log('[DevTools] UIManager.measure result:', { x, y, width, height, pageX, pageY });
      if (width > 0 && height > 0) {
        setHighlightedFrame({ x: pageX, y: pageY, width, height, nodeId });
        DevToolsBridge?.highlightNode?.(nodeId, { x: pageX, y: pageY, width, height });
      } else {
        console.log('[DevTools] Invalid dimensions, clearing highlight');
        setHighlightedFrame(null);
        DevToolsBridge?.clearHighlight?.();
      }
    });
  }, []);

  // Find deepest node at given position
  const findNodeAtPosition = useCallback((x: number, y: number, engine: Engine | null): string | null => {
    console.log('[DevTools] findNodeAtPosition called at:', x, y);
    if (!engine) {
      console.log('[DevTools] No engine');
      return null;
    }

    const receiver = engine.getReceiver();
    if (!receiver) {
      console.log('[DevTools] No receiver');
      return null;
    }

    const tree = receiver.getComponentTree?.();
    if (!tree) {
      console.log('[DevTools] No tree');
      return null;
    }

    // Build frames map synchronously from cached values
    const frames = nodeFramesRef.current;
    console.log('[DevTools] Cached frames count:', frames.size);

    // Find deepest node containing the point
    let deepestNode: string | null = null;
    let smallestArea = Infinity;

    function traverse(node: { id: string; children: Array<{ id: string; children: unknown[] }> }) {
      const frame = frames.get(node.id);
      if (frame) {
        const { x: fx, y: fy, width, height } = frame;
        // Check if point is inside this node's frame
        if (x >= fx && x <= fx + width && y >= fy && y <= fy + height) {
          const area = width * height;
          console.log('[DevTools] Node', node.id, 'contains point, area:', area);
          // Prefer smaller (deeper) nodes
          if (area < smallestArea) {
            smallestArea = area;
            deepestNode = node.id;
          }
        }
      }

      // Traverse children
      for (const child of node.children) {
        traverse(child as { id: string; children: Array<{ id: string; children: unknown[] }> });
      }
    }

    traverse(tree as { id: string; children: Array<{ id: string; children: unknown[] }> });
    console.log('[DevTools] findNodeAtPosition result:', deepestNode);
    return deepestNode;
  }, []);

  // DevTools pause handler - toggle pause for active engine
  useEffect(() => {
    if (!DevToolsBridge) return;

    // Register pause handler with native side
    DevToolsBridge.registerPauseHandler?.();

    // Set up event listener for pause toggle from DevTools UI
    const eventEmitter = new NativeEventEmitter(NativeModules.DevToolsBridge);

    const pauseSubscription = eventEmitter.addListener('onTogglePause', () => {
      // Get the active engine
      const activeEngine = engineMapRef.current.get(activeTabId);
      if (!activeEngine) return;

      // Toggle pause state
      if (activeEngine.isPaused) {
        activeEngine.resume();
        DevToolsBridge?.setEnginePaused?.(false);
      } else {
        activeEngine.pause();
        DevToolsBridge?.setEnginePaused?.(true);
      }
    });

    // Set up event listener for inspect mode toggle
    const inspectSubscription = eventEmitter.addListener('onToggleInspect', () => {
      setIsInspectMode((prev) => {
        const next = !prev;
        DevToolsBridge?.setInspectMode?.(next);

        if (next) {
          // Entering inspect mode - measure all node frames
          const activeEngine = engineMapRef.current.get(activeTabId);
          measureAllNodeFrames(activeEngine ?? null);
        } else {
          // Exiting inspect mode - clear highlight
          setHighlightedFrame(null);
          DevToolsBridge?.clearHighlight?.();
        }

        return next;
      });
    });

    // Set up event listener for node selection from DevTools tree
    const selectSubscription = eventEmitter.addListener('onSelectNode', (data: { nodeId: string }) => {
      // When user selects a node in the DevTools tree, highlight it in the UI
      console.log('[DevTools] onSelectNode event received:', data);
      const activeEngine = engineMapRef.current.get(activeTabId);
      console.log('[DevTools] Active engine:', activeEngine ? 'exists' : 'null', 'tabId:', activeTabId);
      measureAndHighlightNode(data.nodeId, activeEngine ?? null);
    });

    return () => {
      pauseSubscription.remove();
      inspectSubscription.remove();
      selectSubscription.remove();
    };
  }, [activeTabId, measureAndHighlightNode, measureAllNodeFrames]);

  const getEngine = useCallback((tabId: string) => {
    const engine = engineMapRef.current.get(tabId);
    if (!engine) return null;

    const collector = contractCollectorsRef.current.get(tabId);
    return {
      id: engine.id,
      getDiagnostics: () => {
        const diagnostics = engine.getDiagnostics?.() ?? null;
        if (!diagnostics) return null;

        const summary = collector?.summary() ?? null;
        if (!summary) return diagnostics;

        const last = summary.last;
        return {
          ...diagnostics,
          contracts: {
            violations: summary.total,
            lastViolationAt: last?.at ?? null,
            lastViolation: last ? `${last.direction}:${last.kind}:${last.eventName}` : null,
          },
        };
      },
    };
  }, []);

  // 使用显式尺寸替代 absoluteFill，确保容器尺寸正确
  const hasExplicitSize = containerWidth && containerWidth > 0 && containerHeight && containerHeight > 0;

  // 当有显式尺寸时，完全不使用 flex，避免布局冲突
  const tabHostStyle = React.useMemo(() => {
    if (hasExplicitSize) {
      return { width: containerWidth, height: containerHeight, position: 'relative' as const };
    }
    return styles.tabHost;
  }, [hasExplicitSize, containerWidth, containerHeight]);

  // Handle inspect click - find the component at the clicked position
  const handleInspectClick = useCallback((x: number, y: number) => {
    console.log('[DevTools] handleInspectClick at:', x, y);
    const activeEngine = engineMapRef.current.get(activeTabId);
    if (!activeEngine) {
      console.log('[DevTools] No active engine for Inspect click');
      return;
    }

    // Find the deepest node at the clicked position
    const nodeId = findNodeAtPosition(x, y, activeEngine);
    console.log('[DevTools] findNodeAtPosition result:', nodeId);

    if (nodeId) {
      console.log('[DevTools] Selecting node from Inspect:', nodeId);
      // Select this node in DevTools tree (triggers tree selection UI)
      DevToolsBridge?.selectNodeFromInspect?.(nodeId);

      // Highlight the selected node in RN UI
      measureAndHighlightNode(nodeId, activeEngine);
    } else {
      console.log('[DevTools] No node found at position');
    }

    // Exit inspect mode after click
    setIsInspectMode(false);
    DevToolsBridge?.setInspectMode?.(false);
  }, [activeTabId, findNodeAtPosition, measureAndHighlightNode]);

  return (
    <View style={tabHostStyle}>
      {tabs.map((tab) => {
        const isActive = tab.id === activeTabId;
        // 重要：显式尺寸模式下不使用 flex:1，完全依赖父容器的宽高
        const displayStyle = { display: isActive ? 'flex' as const : 'none' as const };
        const tabContainerStyle = hasExplicitSize
          ? { width: containerWidth, height: containerHeight, ...displayStyle }
          : [StyleSheet.absoluteFill, displayStyle];
        return (
          <View
            key={`${tab.id}-${tab.reloadKey ?? 0}`}
            style={tabContainerStyle}
          >
            <ActrTabView
              tab={tab}
              isActive={isActive}
              isLeftPanelVisible={isLeftPanelVisible}
              containerWidth={containerWidth}
              onContractViolation={recordContractViolation}
              onEngineChange={handleEngineChange}
            />
          </View>
        );
      })}

      {typeof EngineMonitorOverlay === 'function' ? (
        <EngineMonitorOverlay tabs={monitorTabs} activeTabId={activeTabId} getEngine={getEngine} />
      ) : null}

      {/* Highlight overlay for selected node (always visible when frame is set) */}
      {highlightedFrame && (
        <View
          style={[
            styles.highlightOverlay,
            {
              left: highlightedFrame.x,
              top: highlightedFrame.y,
              width: highlightedFrame.width,
              height: highlightedFrame.height,
            },
          ]}
          pointerEvents="none"
        >
          <View style={styles.highlightLabel}>
            <Text style={styles.highlightLabelText}>
              #{highlightedFrame.nodeId}
            </Text>
          </View>
        </View>
      )}

      {/* Inspect mode overlay */}
      {isInspectMode && (
        <TouchableOpacity
          style={styles.inspectOverlay}
          activeOpacity={1}
          onPress={(event) => {
            const { locationX, locationY } = event.nativeEvent;
            handleInspectClick(locationX, locationY);
          }}
        >
          <View style={styles.inspectBanner}>
            <Text style={styles.inspectBannerText}>
              Inspect Mode - Click to select element
            </Text>
          </View>
        </TouchableOpacity>
      )}
    </View>
  );
};

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
    backgroundColor: '#0a0a0a',
    minWidth: LAYOUT.CENTER_MIN_WIDTH,
  },
  rightPanel: {
    borderLeftWidth: 1,
    borderLeftColor: LAYOUT.BORDER_COLOR,
    flexShrink: 1,
    minWidth: LAYOUT.RIGHT_PANEL_MIN_WIDTH, // 最小宽度，允许 Yoga 自动收缩
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
  inspectOverlay: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: 'rgba(0, 122, 255, 0.1)',
    zIndex: 9999,
  },
  inspectBanner: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    backgroundColor: 'rgba(0, 122, 255, 0.9)',
    paddingVertical: 8,
    paddingHorizontal: 16,
  },
  inspectBannerText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '600',
    textAlign: 'center',
  },
  highlightOverlay: {
    position: 'absolute',
    borderWidth: 2,
    borderColor: 'rgba(0, 122, 255, 0.9)',
    backgroundColor: 'rgba(0, 122, 255, 0.15)',
    zIndex: 9998,
  },
  highlightLabel: {
    position: 'absolute',
    top: -20,
    left: 0,
    backgroundColor: 'rgba(0, 122, 255, 0.9)',
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 3,
  },
  highlightLabelText: {
    color: '#fff',
    fontSize: 10,
    fontWeight: '600',
  },
});
