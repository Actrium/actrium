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
import React, { useCallback, useRef, useState } from 'react';
import { View, Text, ScrollView, TouchableOpacity, ActivityIndicator, NativeModules, StyleSheet } from 'react-native';
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

// 模块级别的 props 缓存
let cachedValidProps: ActrProps | null = null;

// 布局常量
const LAYOUT = {
  LEFT_PANEL_WIDTH: 240,
  RIGHT_PANEL_WIDTH: 400,
  BORDER_COLOR: '#333',
} as const;

type ActrTabViewProps = {
  tab: TabConfig;
  isActive: boolean;
  isLeftPanelVisible: boolean;
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
    send('HOST_VISIBILITY', { visible: isActive, tabId: tab.id });
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
        currentAdapter = createEngineAdapter(currentEngine, {
          onContractViolation: (v) => onContractViolation?.(tab.id, v),
          permissions: tab.permissions ?? [],
          permissionMode: PERMISSION_MODE,
        });

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

  // 渲染三栏布局
  return (
    <View
      style={{
        flex: 1,
        flexDirection: 'row',
        backgroundColor: '#0a0a0a',
      }}
    >
      {/* 左侧面板 */}
      {isLeftPanelVisible && leftPanel && (
        <View
          style={{
            width: LAYOUT.LEFT_PANEL_WIDTH,
            flex: 1,
            borderRightWidth: 1,
            borderRightColor: LAYOUT.BORDER_COLOR,
          }}
        >
          {leftPanel}
        </View>
      )}

      {/* 中间对话窗口 - flex 自适应 */}
      <View
        style={{
          flex: 1,
          backgroundColor: '#0a0a0a',
        }}
      >
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

      {/* 右侧扩展面板 */}
      {isRightPanelVisible && rightPanel && (
        <View
          style={{
            width: LAYOUT.RIGHT_PANEL_WIDTH,
            flex: 1,
            borderLeftWidth: 1,
            borderLeftColor: LAYOUT.BORDER_COLOR,
          }}
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

  const { tabs = [], currentTabId, isLeftPanelVisible = true } = effectiveProps || {};

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

  return (
    <View style={styles.tabHost}>
      {tabs.map((tab) => {
        const isActive = tab.id === activeTabId;
        return (
          <View
            key={tab.id}
            style={[
              StyleSheet.absoluteFill,
              { display: isActive ? 'flex' : 'none' },
            ]}
          >
            <ActrTabView
              tab={tab}
              isActive={isActive}
              isLeftPanelVisible={isLeftPanelVisible}
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
  );
};

const styles = StyleSheet.create({
  tabHost: {
    flex: 1,
    position: 'relative',
  },
});
