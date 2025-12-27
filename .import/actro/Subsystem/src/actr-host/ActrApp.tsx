/**
 * ActrApp v7 - Single Engine Architecture with Panel Extraction
 * 单 Engine 架构 + 面板提取
 *
 * 改进：
 * - ✅ 单个 Engine 加载统一 bundle
 * - ✅ 通过 extractPanels() 分离左右面板
 * - ✅ Host 控制布局（三栏固定-flex-固定）
 * - ✅ 无需修改 rill 核心代码
 * - ✅ 使用 PanelMarker 组件标记面板边界
 *
 * @version 7.0.0 - Single Engine + PanelMarker
 */
import React, { useCallback, useRef, useState } from 'react';
import { View, Text, ScrollView, TouchableOpacity, ActivityIndicator, NativeModules, StyleSheet } from 'react-native';
import { logger } from '../logger';
import { Engine } from 'rill';
import { hostComponents } from './BundleView';
import { EngineMonitorOverlay, extractPanels } from 'askit';
import type { ContractViolation } from 'askit/contracts';
import { createContractViolationCollector, createHostToGuestSender } from 'askit/contracts';
import { createEngineAdapter } from 'askit/core';
import type { ActrProps, TabConfig } from './types';

const log = logger.child('ActrApp');

const nlog = (msg: string) => log.debug(msg);
const nerror = (msg: string) => log.error(msg);


// 🕐 性能计时工具
const MODULE_LOAD_TIME = Date.now();
class PerfTimer {
  private startTime: number;
  private lastMark: number;
  private marks: Map<string, number> = new Map();
  private tabId: string;

  constructor(tabId: string) {
    this.tabId = tabId;
    this.startTime = Date.now();
    this.lastMark = this.startTime;
  }

  mark(name: string): number {
    const now = Date.now();
    const elapsed = now - this.lastMark;
    const total = now - this.startTime;
    this.marks.set(name, now);
    this.lastMark = now;
    nlog(`⏱️ [Perf:${this.tabId}] ${name}: +${elapsed}ms (total: ${total}ms)`);
    return elapsed;
  }

  since(markName: string): number {
    const markTime = this.marks.get(markName);
    if (!markTime) return 0;
    return Date.now() - markTime;
  }

  total(): number {
    return Date.now() - this.startTime;
  }

  summary() {
    nlog(`⏱️ [Perf:${this.tabId}] === 总耗时: ${this.total()}ms ===`);
  }
}

// 全局计时器 Map（按 tabId 隔离）
const perfTimers = new Map<string, PerfTimer>();

nlog(`✅ [ActrApp-v7] 文件已加载 - Single Engine + PanelMarker 版本 (module load at T+${Date.now() - MODULE_LOAD_TIME}ms)`);
nlog(
  `🔎 [ActrApp-v7] askit(host) exports: EngineMonitorOverlay=${typeof EngineMonitorOverlay}, extractPanels=${typeof extractPanels}`,
);

// 🔴 模块级别的 props 缓存
let cachedValidProps: ActrProps | null = null;
let warnedAskItHostExportsMissing = false;

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
  nlog('🔵 [ActrTabView] 渲染开始, tab=' + tab.id + ', isActive=' + isActive);

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

  React.useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

  // 更新回调：提取面板（使用 ref，避免依赖引起的重建）
  const handleUpdate = useCallback(() => {
    nlog(`[ActrTabView] handleUpdate 被调用, isActive=${isActiveRef.current}, tab=${tab.id}`);
    if (!isActiveRef.current) {
      pendingUpdateRef.current = true;
      nlog(`[ActrTabView] handleUpdate: 非活动状态，跳过`);
      return;
    }
    const eng = engineRef.current;
    if (!eng) {
      nlog(`[ActrTabView] handleUpdate: engineRef.current 为空`);
      return;
    }
    const receiver = eng.getReceiver();
    if (!receiver) {
      nlog(`[ActrTabView] handleUpdate: receiver 为空`);
      return;
    }

    if (typeof extractPanels !== 'function') {
      if (!warnedAskItHostExportsMissing) {
        warnedAskItHostExportsMissing = true;
        nerror('[ActrTabView] extractPanels 不可用：请检查 askit 是否正确解析到 host 入口（react-native 条件导出）');
      }
      return;
    }

    // 检查 receiver 状态 - 🔧 FIX: 如果节点数为 0，跳过本次提取，等待下次回调
    const receiverStats = receiver.getStats?.();
    if (receiverStats) {
      nlog(`[ActrTabView] receiver stats: nodes=${receiverStats.nodeCount}, rootChildren=${receiverStats.rootChildrenCount}, batches=${receiverStats.totalBatches}, ops=${JSON.stringify(receiverStats.totalOps)}`);

      // 🔧 FIX: 关键修复 - 如果还没有节点，说明 Guest 渲染还未完成，跳过本次提取
      // receiver 的 onUpdate 回调会在 applyBatch 后自动触发，所以我们只需要等待
      if (receiverStats.nodeCount === 0) {
        nlog(`[ActrTabView] handleUpdate: receiver 节点数为 0，等待下一次更新回调...`);
        return;
      }
    } else {
      nlog(`[ActrTabView] receiver.getStats() 返回空`);
      return;
    }

    nlog(`[ActrTabView] handleUpdate: 提取面板... tab=${tab.id}`);
    const debugInfo = receiver.getDebugInfo?.();
    if (debugInfo) {
      nlog(`[ActrTabView] debugInfo: nodes=${debugInfo.nodeCount}, rootChildren=[${debugInfo.rootChildren?.join(',')}], appendCalls=${debugInfo.appendCalls}`);
      // Print all node types
      if (debugInfo.nodes && debugInfo.nodes.length > 0) {
        const nodeTypes = debugInfo.nodes.map(n => `${n.id}:${n.type}(${n.childCount})`).join(', ');
        nlog(`[ActrTabView] nodeTypes: ${nodeTypes}`);
      }
    }
    const fullTree = receiver.render();
    nlog(`[ActrTabView] fullTree: ${fullTree === null ? 'NULL' : typeof fullTree}, isEl=${React.isValidElement(fullTree)}`);
    if (React.isValidElement(fullTree)) {
      const treeType = fullTree.type;
      nlog(`[ActrTabView] fullTree.type: ${typeof treeType === 'string' ? treeType : (treeType as any)?.displayName || (treeType as any)?.name || 'unknown'}`);
    }
    const panels = extractPanels(fullTree);

    nlog(`[ActrTabView] 提取结果: left=${!!panels.left}, right=${!!panels.right}, tab=${tab.id}`);
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
      // 创建性能计时器
      const perf = new PerfTimer(tab.id.substring(0, 8));
      perfTimers.set(tab.id, perf);

      try {
        setIsLoading(true);
        setLoadError(null);
        perf.mark('init_start');

        // rill auto-detects JSC sandbox on Apple platforms
        currentEngine = new Engine({
          debug: true,
          logger: {
            log: (...args: unknown[]) => nlog(args.map(a => String(a)).join(' ')),
            warn: (...args: unknown[]) => log.warning(args.map(a => String(a)).join(' ')),
            error: (...args: unknown[]) => nerror(args.map(a => String(a)).join(' ')),
          },
        });
        perf.mark('engine_created');

        currentEngine.register(hostComponents);
        perf.mark('host_components_registered');

        currentAdapter = createEngineAdapter(currentEngine, {
          onContractViolation: (v) => onContractViolation?.(tab.id, v),
          permissions: tab.permissions ?? [],
          permissionMode: PERMISSION_MODE,
        });
        perf.mark('adapter_created');

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
        perf.mark('engine_init_done');
        nlog(`⏱️ [Perf:${tab.id.substring(0, 8)}] === Engine 初始化完成，总耗时: ${perf.total()}ms ===`);
      } catch (e: any) {
        if (cancelled) return;
        nerror(`[ActrTabView] Engine 初始化失败 tab=${tab.id}: ${e?.message}`);
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

    // 如果同一个 tab 的脚本发生变化，重建 Engine 以确保隔离且避免残留状态
    if (loadedUnifiedScriptRef.current && loadedUnifiedScriptRef.current !== script) {
      nlog(`[ActrTabView] unifiedScript 变更，重建 Engine tab=${tab.id}`);
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
      const perf = perfTimers.get(tab.id) || new PerfTimer(tab.id.substring(0, 8));
        nlog(`🔄 [ActrTabView] loadUnifiedBundle 开始 tab=${tab.id}, script长度=${script.length}`);

      try {
        perf.mark('bundle_load_start');
        loadedUnifiedScriptRef.current = script;

        nlog(`🔄 [ActrTabView] 创建 receiver...`);
        eng.createReceiver(handleUpdate);
        perf.mark('receiver_created');

        // loadBundle 内部会调用 initializeRuntime，无需单独调用
        const scriptSize = script.length;
        nlog(`🔄 [ActrTabView] 加载 bundle (${Math.round(scriptSize / 1024)}KB)...`);
        await eng.loadBundle(script, {
          containerHeight: 977,
          containerWidth: 1400,
        });
        perf.mark(`bundle_executed (${Math.round(scriptSize / 1024)}KB)`);
        nlog(`🔄 [ActrTabView] bundle 加载完成`);

        // 有时 receiver 的更新回调可能不会立刻触发，手动提取一次确保面板可见
        nlog(`🔄 [ActrTabView] 手动调用 handleUpdate...`);
        handleUpdate();
        // 容错：有些情况下首帧回调不会立刻触发，补打一轮以确保面板提取
        setTimeout(handleUpdate, 0);
        setTimeout(handleUpdate, 50);
        perf.mark('first_render');

        nlog(`⏱️ [Perf:${tab.id.substring(0, 8)}] === Bundle 加载完成，总耗时: ${perf.total()}ms ===`);

        // Phase 2: 通知 Native 内容已加载完成，可以捕获快照
        try {
          NativeModules.SnapshotModule?.onContentReady?.(tab.id);
          nlog(`📸 [ActrApp] 已通知 Native 内容加载完成: ${tab.id}`);
        } catch (err: any) {
          nlog(`📸 [ActrApp] 通知 SnapshotModule 失败: ${err?.message}`);
        }
      } catch (err: any) {
        nerror(`[ActrTabView] 加载 unified bundle 失败 tab=${tab.id}: ${err?.message}`);
        nerror(`[ActrTabView] 错误堆栈: ${err?.stack?.substring(0, 500)}`);
        setLoadError(err instanceof Error ? err : new Error(String(err)));
        if (eng.isDestroyed) {
          nerror(`[ActrTabView] Engine 已销毁，准备重建 tab=${tab.id}`);
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
        <Text style={{ color: '#888', fontSize: 14, marginTop: 16 }}>初始化 Single Engine...</Text>
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
        >
          <Text style={{ color: '#fff', fontSize: 24, marginBottom: 20, fontWeight: 'bold' }}>
            Canvas ActrApp v7
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

          {/* v7 架构说明 */}
          <View style={{
            marginTop: 20,
            padding: 12,
            backgroundColor: '#1a2a2a',
            borderRadius: 8,
            borderLeftWidth: 4,
            borderLeftColor: '#00aaff'
          }}>
            <Text style={{ color: '#88ddff', fontSize: 12, fontWeight: 'bold', marginBottom: 8 }}>
              🚀 v7 Single Engine + PanelMarker 架构
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11, marginBottom: 4 }}>
              ✅ 单 Engine 加载统一 bundle
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11, marginBottom: 4 }}>
              ✅ PanelMarker 标记面板边界
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11, marginBottom: 4 }}>
              ✅ extractPanels() 提取左右面板
            </Text>
            <Text style={{ color: '#cccccc', fontSize: 11 }}>
              ✅ 内存占用减半（vs 双 Engine）
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
  // 🔴 Props 稳定化
  nlog('📊 [ActrApp-v7] 收到 props, keys: ' + Object.keys(props || {}).join(', '));

  const hasValidTabs = props?.tabs && props.tabs.length > 0;
  let effectiveProps = props;

  if (hasValidTabs) {
    cachedValidProps = { ...props };
    nlog('📊 [ActrApp-v7] ✅ 保存有效 props 到缓存 (tabs: ' + props.tabs.length + ')');
  } else if (cachedValidProps) {
    effectiveProps = cachedValidProps;
    nlog('📊 [ActrApp-v7] ⚠️ 使用缓存的 props (tabs: ' + cachedValidProps.tabs.length + ')');
  }

  const { tabs = [], currentTabId, isLeftPanelVisible = true } = effectiveProps || {};
  nlog('📊 [ActrApp-v7] tabs.length=' + tabs.length + ', currentTabId=' + currentTabId + ', isLeftPanelVisible=' + isLeftPanelVisible);

  if (!tabs.length) {
    nlog('📊 [ActrApp-v7] ⚠️ 没有 tabs，返回空 View');
    return <View style={{ flex: 1, backgroundColor: '#111' }} />;
  }

  const activeTabId = currentTabId ?? tabs[0]!.id;
  nlog('📊 [ActrApp-v7] activeTabId=' + activeTabId);

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

  nlog('📊 [ActrApp] 🎨 准备渲染 JSX, tabs.length=' + tabs.length);

  try {
    const result = (
      <View style={styles.tabHost}>
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          nlog('📊 [ActrApp] 🎨 渲染 tab: ' + tab.id + ', isActive=' + isActive);

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
    nlog('📊 [ActrApp] ✅ JSX 创建成功');
    return result;
  } catch (err: any) {
    nerror('📊 [ActrApp] ❌ JSX 创建失败: ' + err?.message);
    nerror('📊 [ActrApp] Stack: ' + err?.stack?.substring(0, 500));
    return <View style={{ flex: 1, backgroundColor: '#ff0000' }}><Text style={{ color: '#fff' }}>Render Error: {err?.message}</Text></View>;
  }
};

log.notice('✅ 组件已导出');

const styles = StyleSheet.create({
  tabHost: {
    flex: 1,
    position: 'relative',
  },
});
