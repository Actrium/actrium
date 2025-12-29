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
import { View, Text, ScrollView, TouchableOpacity, ActivityIndicator, NativeModules, NativeEventEmitter, StyleSheet, findNodeHandle, UIManager, DeviceEventEmitter } from 'react-native';

// Import RN's Inspector API for accurate view hit testing
// This uses React's internal renderer to find views at a point
const getInspectorDataForViewAtPoint = require('react-native/src/private/inspector/getInspectorDataForViewAtPoint').default as (
  inspectedView: unknown,
  locationX: number,
  locationY: number,
  callback: (viewData: {
    hierarchy: Array<{
      name: string;
      getInspectorData: (findNodeHandle: (component: unknown) => number | null) => {
        measure: (callback: (x: number, y: number, width: number, height: number, left: number, top: number) => void) => void;
        props: Record<string, unknown>;
      };
    }>;
    props: Record<string, unknown>;
    selectedIndex: number;
    frame: { left: number; top: number; width: number; height: number };
    pointerY: number;
    touchedViewTag: number;
    closestInstance: unknown;
  }) => boolean
) => void;
import { logger } from '../logger';
import { installConsoleInterceptor } from '../devtools-console';
import { Engine } from 'rill';

// Install console interceptor to forward logs to DevTools Console panel
installConsoleInterceptor();
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
  setRNInspectorActive?: (active: boolean) => void;
  highlightNode?: (nodeId: string | null, frame: { x: number; y: number; width: number; height: number } | null) => void;
  clearHighlight?: () => void;
  selectNodeFromInspect?: (nodeId: string) => void;
  updateBoxModel?: (data: {
    frame: { x: number; y: number; width: number; height: number };
    margin: { top: number; right: number; bottom: number; left: number };
    padding: { top: number; right: number; bottom: number; left: number };
    border?: { top: number; right: number; bottom: number; left: number };
    style?: {
      position?: string;
      backgroundColor?: string;
      top?: number;
      left?: number;
      right?: number;
      bottom?: number;
      display?: string;
      flexDirection?: string;
      alignItems?: string;
      justifyContent?: string;
      flex?: number;
      opacity?: number;
      overflow?: string;
    };
  }) => void;
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

// Extract margin/padding from style object for box model visualization
function extractBoxModelFromStyle(
  style: Record<string, unknown> | undefined,
  frame: { left: number; top: number; width: number; height: number }
): Parameters<NonNullable<typeof DevToolsBridge>['updateBoxModel']>[0] {
  // Parse numeric value (handles number or "10px" style strings)
  const parseNum = (val: unknown): number => {
    if (typeof val === 'number') return val;
    if (typeof val === 'string') return parseFloat(val) || 0;
    return 0;
  };

  // Extract margin
  const margin = {
    top: parseNum(style?.marginTop ?? style?.marginVertical ?? style?.margin ?? 0),
    right: parseNum(style?.marginRight ?? style?.marginHorizontal ?? style?.margin ?? 0),
    bottom: parseNum(style?.marginBottom ?? style?.marginVertical ?? style?.margin ?? 0),
    left: parseNum(style?.marginLeft ?? style?.marginHorizontal ?? style?.margin ?? 0),
  };

  // Extract padding
  const padding = {
    top: parseNum(style?.paddingTop ?? style?.paddingVertical ?? style?.padding ?? 0),
    right: parseNum(style?.paddingRight ?? style?.paddingHorizontal ?? style?.padding ?? 0),
    bottom: parseNum(style?.paddingBottom ?? style?.paddingVertical ?? style?.padding ?? 0),
    left: parseNum(style?.paddingLeft ?? style?.paddingHorizontal ?? style?.padding ?? 0),
  };

  // Extract border
  const border = {
    top: parseNum(style?.borderTopWidth ?? style?.borderWidth ?? 0),
    right: parseNum(style?.borderRightWidth ?? style?.borderWidth ?? 0),
    bottom: parseNum(style?.borderBottomWidth ?? style?.borderWidth ?? 0),
    left: parseNum(style?.borderLeftWidth ?? style?.borderWidth ?? 0),
  };

  // Extract computed styles
  const computedStyle: NonNullable<Parameters<NonNullable<typeof DevToolsBridge>['updateBoxModel']>[0]['style']> = {};

  if (style?.position) computedStyle.position = String(style.position);
  if (style?.backgroundColor) computedStyle.backgroundColor = String(style.backgroundColor);
  if (style?.top !== undefined) computedStyle.top = parseNum(style.top);
  if (style?.left !== undefined) computedStyle.left = parseNum(style.left);
  if (style?.right !== undefined) computedStyle.right = parseNum(style.right);
  if (style?.bottom !== undefined) computedStyle.bottom = parseNum(style.bottom);
  if (style?.display) computedStyle.display = String(style.display);
  if (style?.flexDirection) computedStyle.flexDirection = String(style.flexDirection);
  if (style?.alignItems) computedStyle.alignItems = String(style.alignItems);
  if (style?.justifyContent) computedStyle.justifyContent = String(style.justifyContent);
  if (style?.flex !== undefined) computedStyle.flex = parseNum(style.flex);
  if (style?.opacity !== undefined) computedStyle.opacity = parseNum(style.opacity);
  if (style?.overflow) computedStyle.overflow = String(style.overflow);

  return {
    frame: { x: frame.left, y: frame.top, width: frame.width, height: frame.height },
    margin,
    padding,
    border,
    style: Object.keys(computedStyle).length > 0 ? computedStyle : undefined,
  };
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
    componentName?: string;
  } | null>(null);

  // Node frames cache for hit testing
  const nodeFramesRef = useRef<Map<string, { x: number; y: number; width: number; height: number }>>(new Map());

  // Bidirectional mapping: viewTag <-> nodeId
  const viewTagToNodeIdRef = useRef<Map<number, string>>(new Map());
  const nodeIdToViewTagRef = useRef<Map<string, number>>(new Map());

  // Build viewTag <-> nodeId mapping and measure all frames
  const measureAllNodeFrames = useCallback((engine: Engine | null) => {
    console.log('[DevTools] measureAllNodeFrames called, engine:', engine ? 'exists' : 'null');
    if (!engine) {
      console.log('[DevTools] No engine for measureAllNodeFrames');
      return;
    }

    const receiver = engine.getReceiver();
    console.log('[DevTools] receiver:', receiver ? 'exists' : 'null');
    if (!receiver) {
      console.log('[DevTools] No receiver for measureAllNodeFrames');
      return;
    }

    const allRefs = receiver.getAllNodeRefs?.();
    console.log('[DevTools] getAllNodeRefs available:', !!receiver.getAllNodeRefs);
    if (!allRefs) {
      console.log('[DevTools] No refs from getAllNodeRefs');
      return;
    }

    console.log('[DevTools] Building viewTag mapping for', allRefs.size, 'nodes');
    const frames = new Map<string, { x: number; y: number; width: number; height: number }>();
    const viewTagToNodeId = new Map<number, string>();
    const nodeIdToViewTag = new Map<string, number>();

    let refCount = 0;
    let viewTagCount = 0;
    allRefs.forEach((ref, nodeId) => {
      if (!ref.current) {
        return;
      }
      refCount++;

      // Get native view tag for this node
      const viewTag = findNodeHandle(ref.current as React.Component);
      if (viewTag) {
        viewTagCount++;
        viewTagToNodeId.set(viewTag, String(nodeId));
        nodeIdToViewTag.set(String(nodeId), viewTag);
        console.log('[DevTools] Mapped viewTag', viewTag, '-> nodeId', nodeId);
      }

      // Try measuring the node
      const component = ref.current as { measure?: (callback: (x: number, y: number, width: number, height: number, pageX: number, pageY: number) => void) => void };
      if (typeof component.measure === 'function') {
        component.measure((x, y, width, height, pageX, pageY) => {
          if (width > 0 && height > 0) {
            frames.set(String(nodeId), { x: pageX, y: pageY, width, height });
          }
        });
      }
    });

    console.log('[DevTools] Refs with .current:', refCount, 'ViewTags found:', viewTagCount);

    // Update mappings
    viewTagToNodeIdRef.current = viewTagToNodeId;
    nodeIdToViewTagRef.current = nodeIdToViewTag;
    console.log('[DevTools] Built viewTag mapping:', viewTagToNodeId.size, 'entries');

    // Update frames after a short delay to let measurements complete
    setTimeout(() => {
      nodeFramesRef.current = frames;
      console.log('[DevTools] Frames cached:', frames.size, 'entries');
    }, 100);
  }, []);

  // Measure single node frame and highlight it
  const measureAndHighlightNode = useCallback((
    nodeId: string,
    engine: Engine | null,
    onMeasured?: (frame: { x: number; y: number; width: number; height: number }) => void
  ) => {
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

    const handleMeasureResult = (x: number, y: number, width: number, height: number, pageX: number, pageY: number) => {
      if (width > 0 && height > 0) {
        const frame = { x: pageX, y: pageY, width, height };
        setHighlightedFrame({ ...frame, nodeId });
        DevToolsBridge?.highlightNode?.(nodeId, frame);
        // Store in nodeFramesRef for future use
        nodeFramesRef.current.set(nodeId, frame);
        // Call callback with measured frame
        onMeasured?.(frame);
      } else {
        console.log('[DevTools] Invalid dimensions, clearing highlight');
        setHighlightedFrame(null);
        DevToolsBridge?.clearHighlight?.();
      }
    };

    // Try using component's measure method directly (more reliable on macOS)
    const component = ref.current as { measure?: (callback: (x: number, y: number, width: number, height: number, pageX: number, pageY: number) => void) => void };
    if (typeof component.measure === 'function') {
      console.log('[DevTools] Using component.measure directly');
      component.measure((x, y, width, height, pageX, pageY) => {
        console.log('[DevTools] component.measure result:', { x, y, width, height, pageX, pageY });
        handleMeasureResult(x, y, width, height, pageX, pageY);
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
      handleMeasureResult(x, y, width, height, pageX, pageY);
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
        console.log('[DevTools] Inspect mode toggled to:', next, 'activeTabId:', activeTabId);
        console.log('[DevTools] engineMapRef keys:', Array.from(engineMapRef.current.keys()));

        if (next) {
          // Entering inspect mode - measure all node frames
          const activeEngine = engineMapRef.current.get(activeTabId);
          console.log('[DevTools] Active engine for inspect:', activeEngine ? 'exists' : 'null');
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

      // Get node props first (for style extraction)
      let nodeProps: Record<string, unknown> | null = null;
      if (activeEngine) {
        const receiver = activeEngine.getReceiver();
        const tree = receiver?.getComponentTree?.();
        if (tree) {
          const findNode = (node: { id: string; props?: Record<string, unknown>; children: unknown[] }): Record<string, unknown> | null => {
            if (String(node.id) === data.nodeId) {
              return node.props ?? null;
            }
            for (const child of node.children) {
              const found = findNode(child as { id: string; props?: Record<string, unknown>; children: unknown[] });
              if (found !== null) return found;
            }
            return null;
          };
          nodeProps = findNode(tree as { id: string; props?: Record<string, unknown>; children: unknown[] });
        }
      }

      // Measure and highlight, then update box model with measured frame
      measureAndHighlightNode(data.nodeId, activeEngine ?? null, (frame) => {
        // Update box model with the freshly measured frame
        const style = nodeProps?.style as Record<string, unknown> | undefined;
        const boxModelData = extractBoxModelFromStyle(style, {
          left: frame.x,
          top: frame.y,
          width: frame.width,
          height: frame.height,
        });
        DevToolsBridge?.updateBoxModel?.(boxModelData);
      });
    });

    // Set up event listener to toggle RN's built-in Element Inspector
    const rnInspectorSubscription = eventEmitter.addListener('onToggleRNInspector', () => {
      // Emit the event that RN's AppContainer listens to
      DeviceEventEmitter.emit('toggleElementInspector');
    });

    // Track RN Inspector state (listen to our own toggle events for state sync)
    let rnInspectorActive = false;
    const rnInspectorStateSubscription = DeviceEventEmitter.addListener('toggleElementInspector', () => {
      rnInspectorActive = !rnInspectorActive;
      DevToolsBridge?.setRNInspectorActive?.(rnInspectorActive);
    });

    return () => {
      pauseSubscription.remove();
      inspectSubscription.remove();
      selectSubscription.remove();
      rnInspectorSubscription.remove();
      rnInspectorStateSubscription.remove();
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

  // Ref to the main content view for Inspector API
  const inspectedViewRef = useRef<View>(null);

  // Handle inspect hover - highlight element without selecting
  const handleInspectHover = useCallback((x: number, y: number) => {
    if (!inspectedViewRef.current) return;

    try {
      getInspectorDataForViewAtPoint(
        inspectedViewRef.current,
        x,
        y,
        (viewData) => {
          try {
            if (viewData.hierarchy && viewData.hierarchy.length > 0) {
              const { frame, selectedIndex, hierarchy } = viewData;
              // Get component name from hierarchy
              const componentName = hierarchy[selectedIndex]?.name || 'Unknown';

              // Try to find Rill node ID - first direct match
              let nodeId = viewTagToNodeIdRef.current.get(viewData.touchedViewTag);

              // If no direct match, find via frame matching
              if (!nodeId) {
                for (const [, rillNodeId] of viewTagToNodeIdRef.current.entries()) {
                  const nodeFrame = nodeFramesRef.current.get(rillNodeId);
                  if (nodeFrame) {
                    if (x >= nodeFrame.x && x <= nodeFrame.x + nodeFrame.width &&
                        y >= nodeFrame.y && y <= nodeFrame.y + nodeFrame.height) {
                      if (!nodeId) {
                        nodeId = rillNodeId;
                      } else {
                        const currentFrame = nodeFramesRef.current.get(nodeId);
                        if (currentFrame && nodeFrame.width * nodeFrame.height < currentFrame.width * currentFrame.height) {
                          nodeId = rillNodeId;
                        }
                      }
                    }
                  }
                }
              }

              if (frame && frame.width > 0 && frame.height > 0) {
                setHighlightedFrame({
                  x: frame.left,
                  y: frame.top,
                  width: frame.width,
                  height: frame.height,
                  nodeId: nodeId || String(viewData.touchedViewTag),
                  componentName,
                });
              }
            }
          } catch {
            // Silently ignore hover errors to prevent noise
          }
          return false;
        }
      );
    } catch {
      // Silently ignore hover errors
    }
  }, []);

  // Handle inspect click - use RN's Inspector API for accurate view detection
  const handleInspectClick = useCallback((x: number, y: number) => {
    console.log('[DevTools] handleInspectClick at:', x, y);

    if (!inspectedViewRef.current) {
      console.log('[DevTools] No inspected view ref');
      return;
    }

    // Use RN's getInspectorDataForViewAtPoint API
    // This is much more reliable than our manual frame calculation
    try {
      getInspectorDataForViewAtPoint(
        inspectedViewRef.current,
        x,
        y,
        (viewData) => {
          try {
            console.log('[DevTools] Inspector API result:', {
              hierarchyLength: viewData.hierarchy?.length ?? 0,
              selectedIndex: viewData.selectedIndex,
              frame: viewData.frame,
              touchedViewTag: viewData.touchedViewTag,
            });

            if (viewData.hierarchy && viewData.hierarchy.length > 0) {
              // Get the frame from the API response - this is accurate!
              const { frame } = viewData;
              console.log('[DevTools] Using Inspector frame:', frame);

              // Update highlight with accurate frame data
              if (frame && frame.width > 0 && frame.height > 0) {
                setHighlightedFrame({
                  x: frame.left,
                  y: frame.top,
                  width: frame.width,
                  height: frame.height,
                  nodeId: String(viewData.touchedViewTag),
                });
                DevToolsBridge?.highlightNode?.(String(viewData.touchedViewTag), {
                  x: frame.left,
                  y: frame.top,
                  width: frame.width,
                  height: frame.height,
                });
              }

              // Find the corresponding Rill node ID using our mapping
              // First try direct match
              console.log('[DevTools] viewTagToNodeId map size:', viewTagToNodeIdRef.current.size);
              console.log('[DevTools] nodeFrames map size:', nodeFramesRef.current.size);
              let nodeId = viewTagToNodeIdRef.current.get(viewData.touchedViewTag);
              console.log('[DevTools] Direct viewTag lookup:', viewData.touchedViewTag, '->', nodeId);

              // If no direct match, walk up the hierarchy to find a Rill node
              if (!nodeId && viewData.hierarchy) {
                console.log('[DevTools] Walking hierarchy to find Rill node...');
                for (let i = viewData.selectedIndex; i < viewData.hierarchy.length; i++) {
                  const item = viewData.hierarchy[i];
                  if (!item) continue;

                  try {
                    const inspectorData = item.getInspectorData(findNodeHandle as (component: unknown) => number | null);
                    if (inspectorData && typeof inspectorData.measure === 'function') {
                      // Get the view tag for this hierarchy item by measuring it
                      inspectorData.measure((mx, my, mw, mh, pageX, pageY) => {
                        // Check if this hierarchy item's view tag is in our mapping
                        // We can't easily get the view tag here, but we can check by component name
                      });
                    }
                  } catch {
                    // Ignore errors
                  }

                  // Check if the component name looks like a Rill component
                  const componentName = item.name;
                  console.log('[DevTools] Hierarchy item:', i, componentName);
                }

                // Alternative: scan all mapped view tags and find one whose frame contains the click point
                const clickX = x;
                const clickY = y;
                for (const [tag, rillNodeId] of viewTagToNodeIdRef.current.entries()) {
                  const frame = nodeFramesRef.current.get(rillNodeId);
                  if (frame) {
                    if (clickX >= frame.x && clickX <= frame.x + frame.width &&
                        clickY >= frame.y && clickY <= frame.y + frame.height) {
                      // This Rill node contains the click point
                      // Use the smallest (most specific) matching node
                      if (!nodeId) {
                        nodeId = rillNodeId;
                      } else {
                        const currentFrame = nodeFramesRef.current.get(nodeId);
                        if (currentFrame && frame.width * frame.height < currentFrame.width * currentFrame.height) {
                          nodeId = rillNodeId;
                        }
                      }
                    }
                  }
                }
                console.log('[DevTools] Found via frame matching:', nodeId);
              }

              if (nodeId) {
                console.log('[DevTools] Found Rill nodeId:', nodeId);
                DevToolsBridge?.selectNodeFromInspect?.(nodeId);

                // Send box model data to DevTools
                if (frame && frame.width > 0 && frame.height > 0) {
                  // Try to get style from viewData.props or hierarchy
                  const style = (viewData.props?.style as Record<string, unknown>) ??
                    (viewData.hierarchy?.[viewData.selectedIndex]?.getInspectorData(
                      findNodeHandle as (component: unknown) => number | null
                    )?.props?.style as Record<string, unknown>);
                  const boxModelData = extractBoxModelFromStyle(style, frame);
                  DevToolsBridge?.updateBoxModel?.(boxModelData);
                }
              } else {
                console.log('[DevTools] No Rill node found for viewTag:', viewData.touchedViewTag);
              }
            }
          } catch (callbackError) {
            console.log('[DevTools] Inspector callback error:', callbackError);
          }
          // Note: Don't exit inspect mode - let user manually exit

          return false; // Don't continue traversing
        }
      );
    } catch (error) {
      console.log('[DevTools] Inspector API error:', error);

      // Fallback to our manual implementation
      const activeEngine = engineMapRef.current.get(activeTabId);
      if (activeEngine) {
        const nodeId = findNodeAtPosition(x, y, activeEngine);
        console.log('[DevTools] Fallback findNodeAtPosition result:', nodeId);

        if (nodeId) {
          DevToolsBridge?.selectNodeFromInspect?.(nodeId);
          measureAndHighlightNode(nodeId, activeEngine);
        }
      }
      // Don't exit inspect mode on error - let user manually exit
    }
  }, [activeTabId, findNodeAtPosition, measureAndHighlightNode]);

  // Handle touch for inspect mode (like RN's InspectorOverlay)
  const findViewForTouchEvent = useCallback((e: { nativeEvent: { locationX: number; locationY: number } }) => {
    const { locationX, locationY } = e.nativeEvent;
    handleInspectHover(locationX, locationY);
  }, [handleInspectHover]);

  const handleStartShouldSetResponder = useCallback((e: { nativeEvent: { locationX: number; locationY: number } }) => {
    findViewForTouchEvent(e);
    return true;
  }, [findViewForTouchEvent]);

  const handleResponderRelease = useCallback((e: { nativeEvent: { locationX: number; locationY: number } }) => {
    const { locationX, locationY } = e.nativeEvent;
    handleInspectClick(locationX, locationY);
  }, [handleInspectClick]);

  return (
    <View style={tabHostStyle} pointerEvents="box-none">
      {/* Content container - ref used for Inspector API */}
      <View
        style={styles.contentContainer}
        ref={inspectedViewRef}
        pointerEvents="box-none"
        collapsable={false}
      >
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
      </View>

      {/* Inspect mode overlay - like RN's InspectorOverlay */}
      {isInspectMode && (
        <View
          style={styles.inspectOverlay}
          onStartShouldSetResponder={handleStartShouldSetResponder}
          onResponderMove={findViewForTouchEvent}
          onResponderRelease={handleResponderRelease}
        >
          {/* Highlight box for selected element */}
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
                  {highlightedFrame.componentName || `#${highlightedFrame.nodeId}`}
                </Text>
              </View>
            </View>
          )}
        </View>
      )}

      {/* Highlight when not in inspect mode (from tree selection) */}
      {!isInspectMode && highlightedFrame && (
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
              {highlightedFrame.componentName || `#${highlightedFrame.nodeId}`}
            </Text>
          </View>
        </View>
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
  contentContainer: {
    flex: 1,
  },
  inspectOverlay: {
    position: 'absolute',
    backgroundColor: 'transparent',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
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
