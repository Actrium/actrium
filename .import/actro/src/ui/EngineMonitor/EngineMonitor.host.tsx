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
   * Alert when background tab ops/s exceeds threshold (default 0.2)
   */
  backgroundOpsPerSecond?: number;
  /**
   * Resource threshold (alert if exceeded)
   */
  timers?: number;
  nodes?: number;
  callbacks?: number;
  /**
   * Max applyBatch duration threshold (ms, alert if exceeded)
   */
  applyDurationMsMax?: number;
  /**
   * Allowed skippedOps (backpressure) threshold (default 0)
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
        const diagnostics = engine ? (engine.getDiagnostics?.() ?? null) : null;
        return {
          tabId: tab.id,
          title: tab.title || 'Untitled',
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
      <TouchableOpacity onPress={() => setVisible(!isVisible)} style={styles.monitorButton}>
        <Text style={styles.monitorButtonText}>{isVisible ? 'Close Monitor' : 'Open Monitor'}</Text>
      </TouchableOpacity>

      {isVisible && (
        <View style={styles.monitorOverlay}>
          <Text style={styles.monitorTitle}>Engine Resource Monitor</Text>
          <Text style={styles.monitorHint}>
            Recommend Guest listen to HOST_VISIBILITY and pause interval/animation/polling when not visible
            to reduce background resource consumption; also report GUEST_SLEEP_STATE for Host diagnosis.
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
                guestSleepingValue == null ? 'Unknown' : guestSleepingValue ? 'Sleeping' : 'Active';
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
                      ...timelinePoints.map((p) =>
                        typeof p.applyDurationMsMax === 'number' ? p.applyDurationMsMax : 0
                      )
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
                typeof attr?.windowMs === 'number'
                  ? Math.max(1, Math.round(attr.windowMs / 1000))
                  : null;
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
                      : '(Undeclared/Compatibility Mode)';
                    const permText =
                      tab.permissions && tab.permissions.length > 0
                        ? tab.permissions.join(', ')
                        : '(None)';
                    return (
                      <>
                        <Text style={styles.monitorRowMeta}>
                          Compliance: contract={contractText} permissions={permText}
                        </Text>
                        {tab.status === 'blocked' && (
                          <Text style={styles.monitorWarn}>
                            ⛔ Blocked: {tab.blockedReason ?? 'Unknown reason'}
                          </Text>
                        )}
                      </>
                    );
                  })()}
                  <Text style={styles.monitorRowMeta}>
                    engineId: {row.engineId ?? '(Uninitialized)'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Resources: timers={resources?.timers ?? 0} nodes={resources?.nodes ?? 0} callbacks=
                    {resources?.callbacks ?? 0}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Activity: ops/s={opsPerSecond.toFixed(2)} batch/s=
                    {(activity?.batchesPerSecond ?? 0).toFixed(2)} lastBatch={ageText} applyMs=
                    {activity?.lastBatch?.applyDurationMs ?? '—'} recvMs={lastApplyMs ?? '—'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Attribution: nodeΔ={nodeDelta ?? '—'} ops={opCountsText} top={topTypesText}
                    {skipped > 0
                      ? `  skippedOps=${skipped}(${skippedCountsText})  skippedTop=${skippedTopTypesText}`
                      : ''}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Attribution (past {attrWindowSec ?? '—'}s): samples={attr?.sampleCount ?? '—'} nodeΔ=
                    {attr?.nodeDelta ?? '—'} totalOps={attr?.total ?? '—'} skipped=
                    {attr?.skipped ?? '—'}
                    {'  '}ops={attrCountsText} top={attrTopTypesText}
                    {typeof attr?.skipped === 'number' && attr.skipped > 0
                      ? `  skippedOps(${attrSkippedCountsText})  skippedTop=${attrSkippedTopTypesText}`
                      : ''}
                    {'  '}worst={attrWorstText}
                  </Text>
                  {timelineHasData && (
                    <View style={styles.sparklineRow}>
                      <View style={styles.sparklineBars}>{renderTimelineBars(timelinePoints)}</View>
                      <Text style={styles.sparklineLegend}>
                        Past {Math.round(timelineWindowMs / 1000)}s: sumOps=
                        {timelinePoints.reduce((s, p) => s + (p.ops ?? 0), 0)} skipped=
                        {timelineSkipped}
                      </Text>
                    </View>
                  )}
                  <Text style={styles.monitorRowMeta}>
                    Guest：sleep={guestSleepingText} lastEvent={guest?.lastEventName ?? '—'} age=
                    {guestAgeText} bytes={guest?.lastPayloadBytes ?? '—'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Host→Guest：lastEvent={hostLastEvent} age={hostAgeText} bytes=
                    {host?.lastPayloadBytes ?? '—'}
                  </Text>
                  <Text style={styles.monitorRowMeta}>
                    Contracts：violations={contractViolationCount} last=
                    {contracts?.lastViolation ?? '—'} age=
                    {contractAgeText}
                  </Text>
                  {skipped > 0 && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Receiver backpressure: skipped={skipped}
                    </Text>
                  )}
                  {skippedOver && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ SkippedOps over budget: check rendering frequency/batch size, consider Guest-side throttling or respond to
                      RECEIVER_BACKPRESSURE
                    </Text>
                  )}
                  {isBackgroundBusy && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Background still producing ops (recommend Guest enter sleep)
                    </Text>
                  )}
                  {guestNotCooperating && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Guest still marked as "active" (recommend responding to HOST_VISIBILITY and reporting GUEST_SLEEP_STATE)
                    </Text>
                  )}
                  {guestNotCooperatingAfterVisibility && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Host sent HOST_VISIBILITY, but Guest has not entered sleep (check if Guest handles visibility correctly/missing reports)
                    </Text>
                  )}
                  {resourceOver && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Resources over budget: timers&lt;={b.timers} nodes&lt;={b.nodes} callbacks&lt;=
                      {b.callbacks} (recommend clearing intervals, avoiding unlimited node growth, releasing callback references)
                    </Text>
                  )}
                  {applyOver && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ ApplyBatch duration too high (recommend reducing batch size/update frequency, or use virtual lists/chunked rendering)
                    </Text>
                  )}
                  {contractViolationCount > 0 && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Contracts violation: check if event names/payloads conform to specification
                    </Text>
                  )}
                  {receiverNodeCount != null && receiverNodeCount > b.nodes && (
                    <Text style={styles.monitorWarn}>
                      ⚠️ Node count too high: consider pagination/virtualization/recycling invisible subtrees
                    </Text>
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
        <View
          style={[
            styles.sparklineBarOps,
            { height: opsH, backgroundColor: skipped ? '#ffcc66' : '#94a3b8' },
          ]}
        />
        <View style={[styles.sparklineBarApply, { height: applyH }]} />
      </View>
    );
  });
}

function formatCounts(counts?: Record<string, number> | null): string {
  if (!counts) return '—';
  const entries = Object.entries(counts).filter(
    (e): e is [string, number] => typeof e[1] === 'number' && e[1] > 0
  );
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
  list?: Array<{
    kind?: 'largest' | 'slowest' | 'mostSkipped' | 'mostGrowth';
    total?: number;
    skipped?: number;
    durationMs?: number;
    nodeDelta?: number;
  }> | null
): string {
  if (!list || list.length === 0) return '—';
  const kindText = (k?: string) => {
    switch (k) {
      case 'largest':
        return 'Largest';
      case 'slowest':
        return 'Slowest';
      case 'mostSkipped':
        return 'Most Skipped';
      case 'mostGrowth':
        return 'Most Growth';
      default:
        return 'Abnormal';
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
