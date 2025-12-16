import React from 'react';
import { ScrollView, StyleSheet, Text, TouchableOpacity, View } from 'react-native';

export type EngineMonitorTab = {
  id: string;
  title?: string;
  contract?: { name: string; version: number } | null;
  permissions?: readonly string[];
  status?: 'ok' | 'compat' | 'blocked';
  blockedReason?: string | null;
};

export type EngineMonitorDiagnostics = {
  resources?: {
    timers?: number;
    nodes?: number;
    callbacks?: number;
  };
  contracts?: {
    violations?: number;
    lastViolationAt?: number | null;
    lastViolation?: string | null;
  };
  activity?: {
    opsPerSecond?: number;
    batchesPerSecond?: number;
    lastBatch?: {
      at?: number;
      applyDurationMs?: number | null;
    } | null;
    timeline?: {
      windowMs?: number;
      bucketMs?: number;
      points?: Array<{
        at?: number;
        ops?: number;
        batches?: number;
        skippedOps?: number;
        applyDurationMsAvg?: number | null;
        applyDurationMsMax?: number | null;
      }>;
    };
  };
  receiver?: {
    lastApply?: {
      skipped?: number;
      durationMs?: number;
      nodeDelta?: number;
      opCounts?: Record<string, number>;
      skippedOpCounts?: Record<string, number>;
      topNodeTypes?: Array<{ type: string; ops: number }>;
      topNodeTypesSkipped?: Array<{ type: string; ops: number }>;
    } | null;
    attribution?: {
      windowMs?: number;
      sampleCount?: number;
      total?: number;
      applied?: number;
      skipped?: number;
      failed?: number;
      durationMs?: number;
      nodeDelta?: number;
      opCounts?: Record<string, number>;
      skippedOpCounts?: Record<string, number>;
      topNodeTypes?: Array<{ type: string; ops: number }>;
      topNodeTypesSkipped?: Array<{ type: string; ops: number }>;
      worstBatches?: Array<{
        kind?: 'largest' | 'slowest' | 'mostSkipped' | 'mostGrowth';
        batchId?: number;
        at?: number;
        total?: number;
        applied?: number;
        skipped?: number;
        failed?: number;
        durationMs?: number;
        nodeDelta?: number;
      }>;
    } | null;
    nodeCount?: number;
    lastRenderAt?: number | null;
    lastRenderDurationMs?: number | null;
  } | null;
  host?: {
    lastEventName?: string | null;
    lastEventAt?: number | null;
    lastPayloadBytes?: number | null;
  };
  guest?: {
    lastEventName?: string | null;
    lastEventAt?: number | null;
    lastPayloadBytes?: number | null;
    sleeping?: boolean | null;
  };
};

export type EngineMonitorBudgets = {
  /**
   * 后台 tab ops/s 超过该阈值就告警（默认 0.2）
   */
  backgroundOpsPerSecond?: number;
  /**
   * 资源阈值（超过则告警）
   */
  timers?: number;
  nodes?: number;
  callbacks?: number;
  /**
   * applyBatch 最大耗时阈值（ms，超过则告警）
   */
  applyDurationMsMax?: number;
  /**
   * 允许的 skippedOps（背压）阈值（默认 0）
   */
  skippedOps?: number;
};

export type EngineMonitorRow = {
  tabId: string;
  title: string;
  isActive: boolean;
  engineId: string | null;
  diagnostics: EngineMonitorDiagnostics | null;
};

export type EngineLike = {
  id: string;
  getDiagnostics?: () => EngineMonitorDiagnostics | null;
};

export type EngineGetter = (tabId: string) => EngineLike | null;

export type EngineMonitorOverlayProps = {
  tabs: readonly EngineMonitorTab[];
  activeTabId: string;
  getEngine: EngineGetter;
  pollIntervalMs?: number;
  budgets?: EngineMonitorBudgets;
  visible?: boolean;
  initialVisible?: boolean;
  onVisibleChange?: (visible: boolean) => void;
};

export function EngineMonitorOverlay({
  tabs,
  activeTabId,
  getEngine,
  pollIntervalMs = 1000,
  budgets,
  visible,
  initialVisible = false,
  onVisibleChange,
}: EngineMonitorOverlayProps): React.ReactElement {
  const [uncontrolledVisible, setUncontrolledVisible] = React.useState(initialVisible);
  const isVisible = visible ?? uncontrolledVisible;
  const setVisible = React.useCallback(
    (next: boolean) => {
      if (visible === undefined) setUncontrolledVisible(next);
      onVisibleChange?.(next);
    },
    [onVisibleChange, visible]
  );

  const [rows, setRows] = React.useState<EngineMonitorRow[]>([]);
  const tabById = React.useMemo(() => new Map(tabs.map((t) => [t.id, t])), [tabs]);

  React.useEffect(() => {
    if (!isVisible) return;
    let cancelled = false;

    const tick = () => {
      if (cancelled) return;
      const nextRows = tabs.map((tab) => {
        const engine = getEngine(tab.id);
        const diagnostics = engine ? engine.getDiagnostics?.() ?? null : null;
        return {
          tabId: tab.id,
          title: tab.title || '未命名',
          isActive: tab.id === activeTabId,
          engineId: engine?.id ?? null,
          diagnostics,
        };
      });
      setRows(nextRows);
    };

    tick();
    const timer = setInterval(tick, pollIntervalMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [activeTabId, getEngine, isVisible, pollIntervalMs, tabs]);

  return (
    <>
      <TouchableOpacity
        onPress={() => setVisible(!isVisible)}
        style={styles.monitorButton}
      >
        <Text style={styles.monitorButtonText}>{isVisible ? '关闭监视器' : '打开监视器'}</Text>
      </TouchableOpacity>

      {isVisible && (
        <View style={styles.monitorOverlay}>
          <Text style={styles.monitorTitle}>Engine 资源监视器</Text>
          <Text style={styles.monitorHint}>
            建议 Guest 监听 HOST_VISIBILITY，并在不可见时暂停 interval/动画/轮询，以降低后台资源消耗；同时上报 GUEST_SLEEP_STATE 以便 Host 诊断。
          </Text>
          <ScrollView style={styles.monitorList} contentContainerStyle={{ paddingBottom: 16 }}>
            {rows.map((row) => {
              const activity = row.diagnostics?.activity;
              const contracts = row.diagnostics?.contracts;
              const resources = row.diagnostics?.resources;
              const receiver = row.diagnostics?.receiver;
              const host = row.diagnostics?.host;
              const guest = row.diagnostics?.guest;

              const lastApply = receiver?.lastApply ?? null;
              const attr = receiver?.attribution ?? null;
              const lastBatchAt = activity?.lastBatch?.at ?? null;
              const ageMs = typeof lastBatchAt === 'number' ? Date.now() - lastBatchAt : null;
              const ageText = ageMs === null ? '—' : `${(ageMs / 1000).toFixed(1)}s`;

              const opsPerSecond = activity?.opsPerSecond ?? 0;
              const defaultBudgets: Required<EngineMonitorBudgets> = {
                backgroundOpsPerSecond: 0.2,
                timers: 50,
                nodes: 5000,
                callbacks: 2000,
                applyDurationMsMax: 50,
                skippedOps: 0,
              };
              const b = { ...defaultBudgets, ...(budgets ?? {}) };

              const isBackgroundBusy = !row.isActive && opsPerSecond > b.backgroundOpsPerSecond;
              const skipped = lastApply?.skipped ?? 0;
              const lastApplyMs = lastApply?.durationMs ?? null;
              const nodeDelta = lastApply?.nodeDelta ?? null;
              const receiverNodeCount = receiver?.nodeCount ?? null;

              const guestAgeMs =
                typeof guest?.lastEventAt === 'number' ? Date.now() - guest.lastEventAt : null;
              const guestAgeText = guestAgeMs === null ? '—' : `${(guestAgeMs / 1000).toFixed(1)}s`;
              const guestSleepingValue = guest?.sleeping;
              const guestSleepingText =
                guestSleepingValue == null ? '未知' : guestSleepingValue ? '睡眠' : '活跃';
              const guestNotCooperating = !row.isActive && guestSleepingValue === false;

              const hostAgeMs =
                typeof host?.lastEventAt === 'number' ? Date.now() - host.lastEventAt : null;
              const hostAgeText = hostAgeMs === null ? '—' : `${(hostAgeMs / 1000).toFixed(1)}s`;
              const hostLastEvent = host?.lastEventName ?? '—';
              const hostRecentlyNotifiedVisibility =
                !row.isActive &&
                host?.lastEventName === 'HOST_VISIBILITY' &&
                typeof hostAgeMs === 'number' &&
                hostAgeMs < 5000;
              const guestNotCooperatingAfterVisibility =
                hostRecentlyNotifiedVisibility && guestSleepingValue === false;

              const contractViolationCount = contracts?.violations ?? 0;
              const contractLastAt = contracts?.lastViolationAt ?? null;
              const contractAgeMs =
                typeof contractLastAt === 'number' ? Date.now() - contractLastAt : null;
              const contractAgeText =
                contractAgeMs === null ? '—' : `${(contractAgeMs / 1000).toFixed(1)}s`;

              const rawTimelinePoints = activity?.timeline?.points ?? [];
              const bucketMs = activity?.timeline?.bucketMs ?? 0;
              const timelinePoints = rawTimelinePoints.slice(-60);
              const timelineWindowMs =
                bucketMs > 0 && timelinePoints.length > 0
                  ? bucketMs * timelinePoints.length
                  : (activity?.timeline?.windowMs ?? 0);
              const timelineSkipped = timelinePoints.reduce(
                (sum, p) => sum + (p.skippedOps ?? 0),
                0
              );
              const timelineHasData = timelinePoints.length > 0;
              const applyMax =
                activity?.lastBatch?.applyDurationMs ??
                (timelinePoints.length > 0
                  ? Math.max(
                      ...timelinePoints.map((p) => (typeof p.applyDurationMsMax === 'number' ? p.applyDurationMsMax : 0))
                    )
                  : null);

              const resourceOver =
                (resources?.timers ?? 0) > b.timers ||
                (resources?.nodes ?? 0) > b.nodes ||
                (resources?.callbacks ?? 0) > b.callbacks;
              const applyOver = typeof applyMax === 'number' && applyMax > b.applyDurationMsMax;
              const skippedOver = timelineSkipped > b.skippedOps || skipped > b.skippedOps;

              const opCountsText = formatCounts(lastApply?.opCounts);
              const topTypesText = formatTopTypes(lastApply?.topNodeTypes);
              const skippedCountsText = formatCounts(lastApply?.skippedOpCounts);
              const skippedTopTypesText = formatTopTypes(lastApply?.topNodeTypesSkipped);

              const attrWindowSec =
                typeof attr?.windowMs === 'number' ? Math.max(1, Math.round(attr.windowMs / 1000)) : null;
              const attrCountsText = formatCounts(attr?.opCounts);
              const attrTopTypesText = formatTopTypes(attr?.topNodeTypes);
              const attrSkippedCountsText = formatCounts(attr?.skippedOpCounts);
              const attrSkippedTopTypesText = formatTopTypes(attr?.topNodeTypesSkipped);
              const attrWorstText = formatWorstBatches(attr?.worstBatches);

              return (
                <View key={row.tabId} style={styles.monitorRow}>
                  <Text style={styles.monitorRowTitle}>
                    {row.isActive ? '▶ ' : ''}
                    {row.title}
                  </Text>
                  <Text style={styles.monitorRowMeta}>tabId: {row.tabId}</Text>
                  {(() => {
                    const tab = tabById.get(row.tabId);
                    if (!tab) return null;
                    const contractText = tab.contract
                      ? `${tab.contract.name}@v${tab.contract.version}`
                      : '（未声明/兼容模式）';
                    const permText =
                      tab.permissions && tab.permissions.length > 0 ? tab.permissions.join(', ') : '（无）';
                    return (
                      <>
                        <Text style={styles.monitorRowMeta}>
                          合规：contract={contractText}  permissions={permText}
                        </Text>
                        {tab.status === 'blocked' && (
                          <Text style={styles.monitorWarn}>⛔ 已阻止运行：{tab.blockedReason ?? '未知原因'}</Text>
                        )}
                      </>
                    );
                  })()}
                  <Text style={styles.monitorRowMeta}>engineId: {row.engineId ?? '（未初始化）'}</Text>
                  <Text style={styles.monitorRowMeta}>
                    资源：timers={resources?.timers ?? 0}  nodes={resources?.nodes ?? 0}  callbacks=
                    {resources?.callbacks ?? 0}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    活动：ops/s={opsPerSecond.toFixed(2)}  batch/s={(activity?.batchesPerSecond ?? 0).toFixed(2)}  lastBatch={ageText}  applyMs=
                    {activity?.lastBatch?.applyDurationMs ?? '—'}  recvMs={lastApplyMs ?? '—'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    归因：nodeΔ={nodeDelta ?? '—'}  ops={opCountsText}  top={topTypesText}
                    {skipped > 0 ? `  skippedOps=${skipped}(${skippedCountsText})  skippedTop=${skippedTopTypesText}` : ''}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    归因（近{attrWindowSec ?? '—'}s）：samples={attr?.sampleCount ?? '—'}  nodeΔ={attr?.nodeDelta ?? '—'}  totalOps={attr?.total ?? '—'}  skipped={attr?.skipped ?? '—'}
                    {'  '}ops={attrCountsText}  top={attrTopTypesText}
                    {typeof attr?.skipped === 'number' && attr.skipped > 0
                      ? `  skippedOps(${attrSkippedCountsText})  skippedTop=${attrSkippedTopTypesText}`
                      : ''}
                    {'  '}worst={attrWorstText}
                  </Text>
                  {timelineHasData && (
                    <View style={styles.sparklineRow}>
                      <View style={styles.sparklineBars}>
                        {renderTimelineBars(timelinePoints)}
                      </View>
                      <Text style={styles.sparklineLegend}>
                        近 {Math.round(timelineWindowMs / 1000)}s：sumOps=
                        {timelinePoints.reduce((s, p) => s + (p.ops ?? 0), 0)}  skipped=
                        {timelineSkipped}
                      </Text>
                    </View>
                  )}
                  <Text style={styles.monitorRowMeta}>
                    Guest：sleep={guestSleepingText}  lastEvent={guest?.lastEventName ?? '—'}  age=
                    {guestAgeText}  bytes={guest?.lastPayloadBytes ?? '—'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Host→Guest：lastEvent={hostLastEvent}  age={hostAgeText}  bytes=
                    {host?.lastPayloadBytes ?? '—'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Contracts：violations={contractViolationCount}  last={contracts?.lastViolation ?? '—'}  age=
                    {contractAgeText}
                  </Text>
                  {skipped > 0 && (
                    <Text style={styles.monitorWarn}>⚠️ Receiver 发生 backpressure：skipped={skipped}</Text>
                  )}
                  {skippedOver && (
                    <Text style={styles.monitorWarn}>⚠️ skippedOps 超预算：请检查渲染频率/批次大小，并考虑 Guest 侧节流或响应 RECEIVER_BACKPRESSURE</Text>
                  )}
                  {isBackgroundBusy && (
                    <Text style={styles.monitorWarn}>⚠️ 后台仍在产出 ops（建议 Guest 进入睡眠）</Text>
                  )}
                  {guestNotCooperating && (
                    <Text style={styles.monitorWarn}>⚠️ Guest 仍标记为“活跃”（建议响应 HOST_VISIBILITY 并上报 GUEST_SLEEP_STATE）</Text>
                  )}
                  {guestNotCooperatingAfterVisibility && (
                    <Text style={styles.monitorWarn}>⚠️ Host 已发送 HOST_VISIBILITY，但 Guest 仍未进入睡眠（优先检查 Guest 是否正确处理可见性/是否遗漏上报）</Text>
                  )}
                  {resourceOver && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ 资源超预算：timers&lt;={b.timers} nodes&lt;={b.nodes} callbacks&lt;={b.callbacks}（建议清理 interval、避免无限节点增长、释放回调引用）
                    </Text>
                  )}
                  {applyOver && (
                    <Text style={styles.monitorWarn}>⚠️ applyBatch 耗时过高（建议降低 batch 体积/更新频率，或使用虚拟列表/分片渲染）</Text>
                  )}
                  {contractViolationCount > 0 && (
                    <Text style={styles.monitorWarn}>⚠️ Contracts 违规：请检查事件名/ payload 是否符合规范</Text>
                  )}
                  {receiverNodeCount != null && receiverNodeCount > b.nodes && (
                    <Text style={styles.monitorWarn}>⚠️ 节点数过高：考虑分页/虚拟化/回收不可见子树</Text>
                  )}
                </View>
              );
            })}
          </ScrollView>
        </View>
      )}
    </>
  );
}

function renderTimelineBars(
  points: ReadonlyArray<{
    ops?: number;
    applyDurationMsMax?: number | null;
    skippedOps?: number;
  }>
): React.ReactElement[] {
  const maxOps = Math.max(1, ...points.map((p) => p.ops ?? 0));
  const maxApply = Math.max(
    1,
    ...points.map((p) => (typeof p.applyDurationMsMax === 'number' ? p.applyDurationMsMax : 0))
  );
  return points.map((p, i) => {
    const ops = p.ops ?? 0;
    const apply = typeof p.applyDurationMsMax === 'number' ? p.applyDurationMsMax : 0;
    const skipped = (p.skippedOps ?? 0) > 0;
    const opsH = Math.round((ops / maxOps) * 28);
    const applyH = Math.round((apply / maxApply) * 28);
    return (
      <View key={i} style={styles.sparklineBar}>
        <View style={[styles.sparklineBarOps, { height: opsH, backgroundColor: skipped ? '#ffcc66' : '#94a3b8' }]} />
        <View style={[styles.sparklineBarApply, { height: applyH }]} />
      </View>
    );
  });
}

function formatCounts(counts?: Record<string, number> | null): string {
  if (!counts) return '—';
  const entries = Object.entries(counts).filter((e): e is [string, number] => typeof e[1] === 'number' && e[1] > 0);
  if (entries.length === 0) return '—';
  return entries
    .sort((a, b) => b[1] - a[1])
    .slice(0, 4)
    .map(([k, v]) => `${k}:${v}`)
    .join(' ');
}

function formatTopTypes(list?: Array<{ type: string; ops: number }> | null): string {
  if (!list || list.length === 0) return '—';
  return list
    .filter((t) => typeof t?.type === 'string' && typeof t?.ops === 'number')
    .slice(0, 3)
    .map((t) => `${t.type}:${t.ops}`)
    .join(' ');
}

function formatWorstBatches(
  list?:
    | Array<{
        kind?: 'largest' | 'slowest' | 'mostSkipped' | 'mostGrowth';
        total?: number;
        skipped?: number;
        durationMs?: number;
        nodeDelta?: number;
      }>
    | null
): string {
  if (!list || list.length === 0) return '—';
  const kindText = (k?: string) => {
    switch (k) {
      case 'largest':
        return '最大';
      case 'slowest':
        return '最慢';
      case 'mostSkipped':
        return '跳过多';
      case 'mostGrowth':
        return '增长多';
      default:
        return '异常';
    }
  };
  return list
    .slice(0, 3)
    .map((b) => {
      const t = typeof b.total === 'number' ? b.total : '—';
      const s = typeof b.skipped === 'number' ? b.skipped : '—';
      const ms = typeof b.durationMs === 'number' ? b.durationMs : '—';
      const d = typeof b.nodeDelta === 'number' ? b.nodeDelta : '—';
      return `${kindText(b.kind)} t=${t} s=${s} ms=${ms} Δ=${d}`;
    })
    .join(' | ');
}

const styles = StyleSheet.create({
  monitorButton: {
    position: 'absolute',
    top: 12,
    right: 12,
    paddingVertical: 8,
    paddingHorizontal: 12,
    backgroundColor: 'rgba(0,0,0,0.6)',
    borderColor: 'rgba(255,255,255,0.2)',
    borderWidth: 1,
    borderRadius: 8,
    zIndex: 999,
  },
  monitorButtonText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '600',
  },
  monitorOverlay: {
    position: 'absolute',
    top: 44,
    right: 12,
    width: 420,
    maxHeight: 520,
    backgroundColor: 'rgba(10,10,10,0.92)',
    borderColor: 'rgba(255,255,255,0.12)',
    borderWidth: 1,
    borderRadius: 12,
    padding: 12,
    zIndex: 1000,
  },
  monitorTitle: {
    color: '#fff',
    fontSize: 14,
    fontWeight: '700',
    marginBottom: 6,
  },
  monitorHint: {
    color: 'rgba(255,255,255,0.7)',
    fontSize: 11,
    lineHeight: 16,
    marginBottom: 10,
  },
  monitorList: {
    flexGrow: 0,
  },
  monitorRow: {
    padding: 10,
    borderRadius: 10,
    backgroundColor: 'rgba(255,255,255,0.05)',
    borderColor: 'rgba(255,255,255,0.08)',
    borderWidth: 1,
    marginBottom: 10,
  },
  monitorRowTitle: {
    color: '#fff',
    fontSize: 13,
    fontWeight: '700',
    marginBottom: 6,
  },
  monitorRowMeta: {
    color: 'rgba(255,255,255,0.75)',
    fontSize: 11,
    lineHeight: 16,
  },
  monitorWarn: {
    color: '#ffcc66',
    fontSize: 11,
    lineHeight: 16,
    marginTop: 6,
  },
  sparklineRow: {
    marginTop: 8,
    marginBottom: 2,
  },
  sparklineBars: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    height: 30,
    marginBottom: 4,
    opacity: 0.9,
  },
  sparklineBar: {
    width: 4,
    height: 30,
    position: 'relative',
    justifyContent: 'flex-end',
    marginRight: 1,
  },
  sparklineBarOps: {
    width: 4,
    borderRadius: 2,
  },
  sparklineBarApply: {
    position: 'absolute',
    bottom: 0,
    width: 4,
    borderRadius: 2,
    backgroundColor: '#60a5fa',
    opacity: 0.5,
  },
  sparklineLegend: {
    color: 'rgba(255,255,255,0.6)',
    fontSize: 10,
    lineHeight: 14,
  },
});
