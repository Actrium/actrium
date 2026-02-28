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
import { View, Text, ScrollView, TouchableOpacity, Pressable, ActivityIndicator, NativeModules, NativeEventEmitter, StyleSheet, findNodeHandle, UIManager, DeviceEventEmitter, LayoutChangeEvent } from 'react-native';

type GetInspectorDataForViewAtPoint = (
  inspectedView: unknown,
  locationX: number,
  locationY: number,
  callback: (viewData: {
    hierarchy: Array<{
      name: string;
      getInspectorData: (findNodeHandle: (component: unknown) => number | null) => {
        measure: (
          callback: (x: number, y: number, width: number, height: number, left: number, top: number) => void
        ) => void;
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

// Import RN's Inspector API for accurate view hit testing (best-effort).
// react-native-macos 的内部路径在不同版本间会变；这里必须做兼容，否则会因为 `.default` 取值导致
// “Cannot read property 'default' of undefined” 直接把整个 ActrApp 渲染打爆（用户点击触发时最常见）。
function resolveGetInspectorDataForViewAtPoint(): GetInspectorDataForViewAtPoint | null {
  // 注意：Metro 不允许 `require(variable)`，这里必须保持静态 require。
  try {
    // react-native-macos: src/private/devsupport/devmenu/elementinspector/getInspectorDataForViewAtPoint
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('react-native/src/private/devsupport/devmenu/elementinspector/getInspectorDataForViewAtPoint') as unknown;
    const fn = (mod as any)?.default ?? mod;
    if (typeof fn === 'function') return fn as GetInspectorDataForViewAtPoint;
  } catch {}

  return null;
}

const getInspectorDataForViewAtPoint: GetInspectorDataForViewAtPoint | null =
  resolveGetInspectorDataForViewAtPoint();
import { logger } from '../logger';
import { installConsoleInterceptor } from '../devtools-console';
import { Engine } from 'rill/host';

// Install console interceptor to forward logs to DevTools Console panel
installConsoleInterceptor();
import { hostComponents } from './BundleView';
import type { ContractViolation } from 'askit/contracts';
import { createContractViolationCollector, createHostToGuestSender } from 'askit/contracts';
import { createEngineAdapter } from 'askit/host';
import type { ActrProps, TabConfig } from './types';

/**
 * Extract panels from React element tree by __panelId attribute
 * Searches for nodes with __panelId="left", __panelId="right", and __panelId="main"
 */
function extractPanelsByPanelId(
  element: React.ReactNode
): { left: React.ReactElement | null; right: React.ReactElement | null; main: React.ReactElement | null } {
  if (!element || !React.isValidElement(element)) {
    return { left: null, right: null, main: null };
  }

  const result: { left: React.ReactElement | null; right: React.ReactElement | null; main: React.ReactElement | null } = {
    left: null,
    right: null,
    main: null,
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
    if (panelId === 'main') {
      result.main = props.children as React.ReactElement;
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
const nlog = (msg: string) => { try { NativeModules.NativeLogger?.log?.(msg); } catch {} };

// DevTools message logging helper
const DevToolsBridge = NativeModules.DevToolsBridge as {
  logMessage?: (direction: string, event: string, payload: string | null, tabId: string | null) => void;
  logRenderOp?: (operationType: string, componentType: string | null, nodeId: string | null, payload: string | null, tabId: string | null) => void;
  setEnginePaused?: (isPaused: boolean) => void;
  registerPauseHandler?: () => void;
  updateComponentTree?: (treeJson: string | null, tabId: string | null) => void;
  updateEngineMonitor?: (snapshotJson: string | null) => void;
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
  isOverviewMode?: boolean;
  containerWidth?: number;
  containerHeight?: number;
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
  isOverviewMode,
  containerWidth: propsContainerWidth,
  containerHeight: propsContainerHeight,
  onContractViolation,
  onEngineChange,
}) => {
  nlog(`[ActrTabView] render: tabId=${tab.id?.substring(0,8)}, isActive=${isActive}, isLeftPanelVisible=${isLeftPanelVisible}, script=${tab.unifiedScript ? `${tab.unifiedScript.length}chars` : 'nil'}`);
  const gate = evaluateTabGate(tab);
  const [layoutSize, setLayoutSize] = useState<{ width: number; height: number } | null>(null);
  const handleLayout = useCallback((event: LayoutChangeEvent) => {
    const { width, height } = event.nativeEvent.layout;
    if (width > 0 && height > 0) {
      setLayoutSize(prev => {
        if (prev && Math.abs(prev.width - width) < 1 && Math.abs(prev.height - height) < 1) {
          return prev;
        }
        return { width, height };
      });
    }
  }, []);

  const effectiveWidth = layoutSize?.width ?? propsContainerWidth;
  const effectiveHeight = layoutSize?.height ?? propsContainerHeight;
  const isCompactMode =
    !!isOverviewMode ||
    (typeof effectiveWidth === 'number' && effectiveWidth > 0 && effectiveWidth < 520) ||
    (typeof effectiveHeight === 'number' && effectiveHeight > 0 && effectiveHeight < 360);

  // 右侧面板可见性（按 tab 维度保存）
  const [isRightPanelVisible, setIsRightPanelVisible] = useState(tab.rightPanelDefaultVisible ?? true);

  // Engine 状态（按 tab 维度隔离）
  const [engine, setEngine] = useState<Engine | null>(null);
  const [isLoading, setIsLoading] = useState(isActive);
  const [loadError, setLoadError] = useState<Error | null>(null);
  const [leftPanel, setLeftPanel] = useState<React.ReactElement | null>(null);
  const [rightPanel, setRightPanel] = useState<React.ReactElement | null>(null);
  const [mainPanel, setMainPanel] = useState<React.ReactElement | null>(null);

  const engineRef = useRef<Engine | null>(null);
  const adapterRef = useRef<{ dispose: () => void } | null>(null);
  const loadedUnifiedScriptRef = useRef<string | null>(null);
  const initInProgressRef = useRef(false);
  const initTokenRef = useRef(0);
  const initTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const unmountedRef = useRef(false);
  const isActiveRef = useRef(isActive);
  const pendingUpdateRef = useRef(false);
  const lastBatchesRef = useRef<number>(0);

  React.useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

  // 显示右侧面板：用户开启可见性即显示（即使内容为空也显示占位）
  const showRightPanel = isRightPanelVisible;

  // 优化：使用固定宽度 + flexShrink，让 Yoga 自动处理收缩
  // 不再依赖 propsContainerWidth 计算，避免 re-render
  const leftWidth = isLeftPanelVisible ? LAYOUT.LEFT_PANEL_WIDTH : 0;
  const rightWidth = showRightPanel ? LAYOUT.RIGHT_PANEL_WIDTH : 0;

  // 更新回调：提取面板
  const handleUpdate = useCallback(() => {
    nlog(`[ActrTabView] handleUpdate called: isActive=${isActiveRef.current}, hasEngine=${!!engineRef.current}`);
    if (!isActiveRef.current) {
      pendingUpdateRef.current = true;
      nlog(`[ActrTabView] handleUpdate: not active, queued`);
      return;
    }
    const eng = engineRef.current;
    if (!eng) { nlog(`[ActrTabView] handleUpdate: no engine`); return; }
    const receiver = eng.getReceiver();
    if (!receiver) { nlog(`[ActrTabView] handleUpdate: no receiver`); return; }

    const receiverStats = receiver.getStats?.();
    if (!receiverStats || receiverStats.nodeCount === 0) return;

    // 只有当 batches 变化时才更新面板
    const currentBatches = receiverStats.totalBatches;
    if (currentBatches === lastBatchesRef.current) return;
    lastBatchesRef.current = currentBatches;

    const fullTree = receiver.render();
    const panels = extractPanelsByPanelId(fullTree);
    nlog(`[ActrTabView] handleUpdate: leftPanel=${!!panels.left}, rightPanel=${!!panels.right}, mainPanel=${!!panels.main}, fullTree=${fullTree ? 'exists' : 'null'}`);
    setLeftPanel(panels.left);
    setRightPanel(panels.right);
    setMainPanel(panels.main);

    // Update DevTools component tree
    updateDevToolsComponentTree(eng, tab.id);
  }, [tab.id]);

  // 组件卸载时清理（tab 关闭）
  React.useEffect(() => {
    return () => {
      unmountedRef.current = true;
      if (initTimeoutRef.current) {
        clearTimeout(initTimeoutRef.current);
        initTimeoutRef.current = null;
      }
      initInProgressRef.current = false;
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
    nlog(`[ActrTabView] Engine init effect: isActive=${isActive}, gate=${gate.status}, hasEngine=${!!engineRef.current}, inProgress=${initInProgressRef.current}, tabId=${tab.id?.substring(0,8)}`);
    if (!isActive) return;
    if (gate.status === 'blocked') return;
    if (engineRef.current) {
      if (isLoading) setIsLoading(false);
      return;
    }
    if (initInProgressRef.current) return;

    initInProgressRef.current = true;
    const token = ++initTokenRef.current;
    let currentEngine: Engine | null = null;
    let currentAdapter: { dispose: () => void } | null = null;

    async function init() {
      try {
        const t0 = Date.now();
        nlog(`[ActrTabView] Engine init starting... t=${t0}`);
        setIsLoading(true);
        setLoadError(null);

        if (initTimeoutRef.current) {
          clearTimeout(initTimeoutRef.current);
          initTimeoutRef.current = null;
        }
        initTimeoutRef.current = setTimeout(() => {
          if (unmountedRef.current) return;
          if (initTokenRef.current !== token) return;
          if (engineRef.current) return;
          nerror(`[ActrTabView] Engine init timeout (>8s), tabId=${tab.id?.substring(0,8)}`);
          initTokenRef.current += 1; // invalidate this attempt
          initInProgressRef.current = false;
          setLoadError(new Error('Engine 初始化超时，请重试'));
          setIsLoading(false);
        }, 8000);

        currentEngine = new Engine({
          debug: false,
          logger: {
            log: (...args: unknown[]) => {
              // Engine log messages (only emitted when debug=true or unconditionally)
            },
            warn: (...args: unknown[]) => log.warning(args.map(a => String(a)).join(' ')),
            error: (...args: unknown[]) => nerror(args.map(a => String(a)).join(' ')),
          },
        });
        currentEngine.register(hostComponents);

        // Generic message handler with DevTools logging
        const unsubscribeMessage = currentEngine.on('message', async (msg: { event: string; payload?: unknown }) => {
          // Log all Guest → Host messages to DevTools
          nlog(`[ActrTabView] 📩 Guest message: event=${msg.event} payload=${JSON.stringify(msg.payload)}`);
          logDevToolsMessage('guestToHost', msg.event, msg.payload, tab.id);

          // Handle TOGGLE_RIGHT_PANEL from guest
          if (msg.event === 'TOGGLE_RIGHT_PANEL') {
            nlog(`[ActrTabView] 🔄 TOGGLE_RIGHT_PANEL received, toggling right panel`);
            setIsRightPanelVisible(prev => !prev);
            return;
          }

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
            // Extract response headers for DevTools
            const responseHeaders: Record<string, string> = {};
            response.headers.forEach((value, key) => {
              responseHeaders[key] = value;
            });
            const responsePayload = {
              requestId,
              ok: response.ok,
              status: response.status,
              headers: responseHeaders,
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

        if (unmountedRef.current || initTokenRef.current !== token) {
          currentAdapter?.dispose();
          currentEngine?.destroy();
          return;
        }

        engineRef.current = currentEngine;
        adapterRef.current = currentAdapter;
        setEngine(currentEngine);
        onEngineChange?.(tab.id, currentEngine);
        nlog(`[ActrTabView] Engine created successfully, tabId=${tab.id?.substring(0,8)}, elapsed=${Date.now() - t0}ms`);
        setIsLoading(false);
      } catch (e: any) {
        if (unmountedRef.current || initTokenRef.current !== token) return;
        nerror(`[ActrTabView] init failed: ${e?.message}`);
        setLoadError(e instanceof Error ? e : new Error(String(e)));
        setIsLoading(false);
      } finally {
        if (initTimeoutRef.current) {
          clearTimeout(initTimeoutRef.current);
          initTimeoutRef.current = null;
        }
        if (initTokenRef.current === token) {
          initInProgressRef.current = false;
        }
      }
    }

    init();

    return () => {
      // 不在切换 tab 时取消 init，让 Engine 在后台完成，避免反复卡住
      // 清理 timeout，避免泄漏
      if (initTimeoutRef.current) {
        clearTimeout(initTimeoutRef.current);
        initTimeoutRef.current = null;
      }
    };
  }, [isActive, tab.id, tab.unifiedScript, gate.status, isLoading, onContractViolation, onEngineChange, tab.permissions]);

  // 动态更新 Guest config：让 `useConfig()` 能及时拿到 containerSize/isOverviewMode
  // 这对 Overview Card 的 compact 渲染以及状态保持都很关键。
  React.useEffect(() => {
    if (!engine) return;
    if (engine.isDestroyed) return;
    if (!engine.isLoaded) return;
    engine.updateConfig({
      containerWidth: effectiveWidth ?? 1400,
      containerHeight: effectiveHeight ?? 977,
      isOverviewMode: !!isCompactMode,
    } as any);
  }, [engine, effectiveWidth, effectiveHeight, isCompactMode]);

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
    nlog(`[ActrTabView] Bundle load effect: hasEngine=${!!engine}, isActive=${isActive}, hasScript=${!!unifiedScript}, scriptLen=${unifiedScript?.length ?? 0}, tabId=${tab.id?.substring(0,8)}`);
    if (!unifiedScript) { nlog(`[ActrTabView] No unifiedScript, skipping bundle load`); return; }

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
        const tBundleStart = Date.now();
        nlog(`[ActrTabView] loadUnifiedBundle starting, scriptLen=${script.length}, t=${tBundleStart}`);
        loadedUnifiedScriptRef.current = script;
        eng.createReceiver();
        eng.on('update', handleUpdate);
        nlog(`[ActrTabView] createReceiver done, elapsed=${Date.now() - tBundleStart}ms`);
        await eng.loadBundle(script, {
          tabId: tab.id,
          title: tab.title,
          backgroundColor: tab.backgroundColor,
          containerWidth: effectiveWidth ?? 1400,
          containerHeight: effectiveHeight ?? 977,
          isOverviewMode: !!isCompactMode,
        });
        nlog(`[ActrTabView] loadBundle done, elapsed=${Date.now() - tBundleStart}ms`);

        // 确保面板提取
        nlog(`[ActrTabView] Bundle loaded, calling handleUpdate`);
        handleUpdate();

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
      <View onLayout={handleLayout} style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
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
      <View onLayout={handleLayout} style={{ flex: 1, alignItems: 'center', justifyContent: 'center', backgroundColor: '#0a0a0a' }}>
        <ActivityIndicator size="large" color="#007AFF" />
        <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>初始化 Single Engine (usePanels mode)...</Text>
      </View>
    );
  }

  // 加载错误
  if (isActive && loadError) {
    return (
      <View onLayout={handleLayout} style={{ flex: 1, alignItems: 'center', justifyContent: 'center', padding: 16, backgroundColor: '#1a1a1a' }}>
        <Text style={{ color: '#ff6b6b', fontSize: 18, fontWeight: '600', marginBottom: 12 }}>
          Engine 初始化失败
        </Text>
        <Text style={{ color: '#ccc', fontSize: 14, textAlign: 'center', marginBottom: 8 }}>{loadError.message}</Text>
      </View>
    );
  }

  // Overview 卡片模式：只渲染 Guest 的 compact 视图（左侧面板占满）
  if (isCompactMode) {
    return (
      <View onLayout={handleLayout} style={{ flex: 1, backgroundColor: '#0a0a0a' }}>
        {leftPanel || (
          <View style={{ flex: 1, justifyContent: 'center', alignItems: 'center' }}>
            <Text style={{ color: '#888', fontSize: 12 }}>Loading...</Text>
          </View>
        )}
      </View>
    );
  }

  return (
    <View onLayout={handleLayout} style={threeColumnStyle}>
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
          {leftPanel || (
            <View style={{ flex: 1, padding: 12, backgroundColor: '#1a1a2a' }}>
              <Text style={{ color: '#888', fontSize: 12, marginBottom: 8 }}>📁 左侧面板</Text>
              <Text style={{ color: '#666', fontSize: 11 }}>等待 Engine 内容...</Text>
            </View>
          )}
        </View>
      )}

      {/* 中间对话窗口 - flex 自适应 */}
      <View style={[styles.centerColumn]}>
        <View style={styles.mainPanelContainer}>
          <Pressable
            onPress={() => setIsRightPanelVisible((prev) => !prev)}
            style={({ pressed }) => [
              styles.toggleRightPanelButton,
              pressed ? { transform: [{ scale: 0.98 }], opacity: 0.9 } : null,
            ]}
          >
            <Text style={styles.toggleRightPanelButtonText}>Toggle 右栏面板</Text>
          </Pressable>
          {mainPanel || (
            <View style={{ flex: 1, justifyContent: 'center', alignItems: 'center' }}>
              <Text style={{ color: '#FF9800', fontSize: 24, fontWeight: 'bold' }}>main panel</Text>
            </View>
          )}
        </View>
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
          {rightPanel || (
            <View style={{ flex: 1, padding: 12, backgroundColor: '#1a2a1a' }}>
              <Text style={{ color: '#888', fontSize: 12, marginBottom: 8 }}>📊 右侧面板</Text>
              <Text style={{ color: '#666', fontSize: 11 }}>等待 Engine 内容...</Text>
            </View>
          )}
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

  const { tabs = [], currentTabId, isLeftPanelVisible = true, containerWidth, containerHeight, isOverviewMode } = effectiveProps || {};

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

  // 推送引擎监控快照到原生 DevTools（用于 Monitor 面板）
  useEffect(() => {
    if (!DevToolsBridge?.updateEngineMonitor) return;

    let cancelled = false;
    const pollIntervalMs = 1000;

    const tick = () => {
      if (cancelled) return;
      try {
        // DevTools Monitor 只针对当前 Tab（多 Tab 总览由独立 Task Manager 承载）
        const activeTab = monitorTabs.find((t) => t.id === activeTabId) ?? monitorTabs[0];
        if (!activeTab) return;

        const engine = getEngine(activeTab.id);
        const rawDiagnostics = engine ? (engine.getDiagnostics?.() as any) : null;

        // 收敛 timeline points 体积，避免桥接 payload 过大
        const timeline = rawDiagnostics?.activity?.timeline;
        const points: any[] | undefined = Array.isArray(timeline?.points) ? timeline.points : undefined;
        const diagnostics = points && points.length > 60
          ? {
              ...rawDiagnostics,
              activity: {
                ...rawDiagnostics.activity,
                timeline: {
                  ...timeline,
                  points: points.slice(-60),
                },
              },
            }
          : rawDiagnostics;

        const row = {
          tabId: activeTab.id,
          title: activeTab.title || 'Untitled',
          isActive: true,
          engineId: engine?.id ?? null,
          status: (activeTab as any).status ?? null,
          blockedReason: (activeTab as any).blockedReason ?? null,
          contract: (activeTab as any).contract ?? null,
          permissions: (activeTab as any).permissions ?? null,
          diagnostics,
        };

        DevToolsBridge.updateEngineMonitor?.(
          JSON.stringify({
            timestamp: Date.now(),
            activeTabId,
            rows: [row],
          })
        );
      } catch {
        // Ignore DevTools monitor update errors
      }
    };

    tick();
    const timer = setInterval(tick, pollIntervalMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [activeTabId, getEngine, monitorTabs]);

  // 🔑 Always use flex:1 layout — RCTRootView frame is updated by native on resize,
  // flex children automatically fill the available space without needing explicit dimensions.
  // (containerWidth/containerHeight are still passed to Engine config for initial setup)
  const tabHostStyle = styles.tabHost;

  // Ref to the main content view for Inspector API
  const inspectedViewRef = useRef<View>(null);

  // Handle inspect hover - highlight element without selecting
  const handleInspectHover = useCallback((x: number, y: number) => {
    if (!inspectedViewRef.current) return;
    if (!getInspectorDataForViewAtPoint) return;

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
    if (!getInspectorDataForViewAtPoint) {
      console.log('[DevTools] Inspector API not available, falling back to manual hit testing');
      const activeEngine = engineMapRef.current.get(activeTabId);
      if (activeEngine) {
        const nodeId = findNodeAtPosition(x, y, activeEngine);
        if (nodeId) {
          DevToolsBridge?.selectNodeFromInspect?.(nodeId);
          measureAndHighlightNode(nodeId, activeEngine);
        }
      }
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
          const displayStyle = { display: isActive ? 'flex' as const : 'none' as const };
          const tabContainerStyle = [StyleSheet.absoluteFill, displayStyle];
          return (
            <View
              key={`${tab.id}-${tab.reloadKey ?? 0}`}
              style={tabContainerStyle}
            >
              <ActrTabView
                tab={tab}
                isActive={isActive}
                isLeftPanelVisible={isLeftPanelVisible}
                isOverviewMode={isOverviewMode}
                containerWidth={containerWidth}
                containerHeight={containerHeight}
                onContractViolation={recordContractViolation}
                onEngineChange={handleEngineChange}
              />
            </View>
          );
        })}
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
  mainPanelContainer: {
    flex: 1,
    position: 'relative',
  },
  toggleRightPanelButton: {
    position: 'absolute',
    left: 10,
    top: 10,
    zIndex: 10,
    backgroundColor: 'rgba(33, 150, 243, 0.95)',
    paddingHorizontal: 10,
    paddingVertical: 6,
    borderRadius: 6,
  },
  toggleRightPanelButtonText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '700',
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
