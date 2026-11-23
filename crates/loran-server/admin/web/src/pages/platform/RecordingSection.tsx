import { usePlatformData, filterFields, PlatformDataGuard } from "./shared";
import { ServicePageLayout, ConfigSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import type { ResolvedField } from "../../lib/api";

/* ── helpers ─────────────────────────────────────────────────── */

function fieldValue(fields: ResolvedField[], key: string): string | null {
  const f = fields.find((f) => f.key === key);
  if (!f) return null;
  const v = f.effective_value.trim();
  if (!v || v === "none" || v === "None") return null;
  return v;
}

function sinkType(uri: string | null): keyof typeof SINK_STYLE {
  if (uri?.startsWith("file://")) return "file";
  if (uri?.startsWith("otlp+")) return "otlp";
  return "stdout";
}

function shortUri(uri: string, maxLen: number): string {
  if (uri.length <= maxLen) return uri;
  return uri.slice(0, maxLen - 1) + "\u2026";
}

/* ── channel palette ─────────────────────────────────────────── */

const CHANNELS = [
  {
    id: "observability",
    label: "Observability",
    sub: "Latency \u00b7 health \u00b7 metrics",
    fill: "#dbeafe",
    stroke: "#3b82f6",
    text: "#1e40af",
  },
  {
    id: "audit",
    label: "Audit",
    sub: "Config changes \u00b7 access",
    fill: "#fef3c7",
    stroke: "#d97706",
    text: "#92400e",
  },
  {
    id: "security",
    label: "Security",
    sub: "Auth failures \u00b7 abuse",
    fill: "#fce7f3",
    stroke: "#db2777",
    text: "#9d174d",
  },
  {
    id: "operations",
    label: "Operations",
    sub: "Restarts \u00b7 deployments",
    fill: "#e0e7ff",
    stroke: "#6366f1",
    text: "#3730a3",
  },
] as const;

/* ── Sink target styles ──────────────────────────────────────── */

const SINK_STYLE = {
  file:   { fill: "#ecfdf5", stroke: "#059669", text: "#065f46", label: "File Sink",   lineColor: "#059669" },
  otlp:   { fill: "#ede9fe", stroke: "#7c3aed", text: "#4c1d95", label: "OTLP Sink",   lineColor: "#7c3aed" },
  stdout: { fill: "#f3f4f6", stroke: "#9ca3af", text: "#374151", label: "stdout",      lineColor: "#9ca3af" },
} as const;

/* ── The diagram ─────────────────────────────────────────────── */

function RecordingDiagram({ fields }: { fields: ResolvedField[] }) {
  const globalSink = fieldValue(fields, "recording.sink");

  /* Resolve each channel */
  const resolved = CHANNELS.map((ch) => {
    const perChannel = fieldValue(fields, `recording.${ch.id}.sink`);
    const uri = perChannel ?? globalSink;
    const type = sinkType(uri);
    const isOverride = perChannel !== null;
    const filter = fieldValue(fields, `recording.${ch.id}.filter`) ?? "—";
    return { ...ch, uri, type, isOverride, filter };
  });

  const destinations = [...new Set(resolved.map((ch) => ch.uri))];

  /* ── Layout ── */
  const W = 620;
  const chW = 128;
  const chH = 50;
  const chGap = 16;
  const N = CHANNELS.length;
  const totalChW = N * chW + (N - 1) * chGap;
  const chLeft = (W - totalChW) / 2;

  const ingestTop = 6;
  const chY = 34;
  const routeTop = chY + chH;
  const sinkY = routeTop + 56;
  const snkH = 56;
  const H = sinkY + snkH + 8;

  const snkGap = 16;
  const snkW = destinations.length > 2 ? 128 : 200;
  const totalSnkW = destinations.length * snkW + (destinations.length - 1) * snkGap;
  const snkLeft = (W - totalSnkW) / 2;

  const chCx = (i: number) => chLeft + i * (chW + chGap) + chW / 2;
  const snkCx = (i: number) => snkLeft + i * (snkW + snkGap) + snkW / 2;

  const sinkCxMap = new Map(destinations.map((uri, i) => [uri, snkCx(i)]));

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      role="img"
      aria-label="Configured routing from recording channels to their destinations"
      className="max-w-3xl mx-auto"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <marker id="rd-gray" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#94a3b8" />
        </marker>
        {Object.entries(SINK_STYLE).map(([type, style]) => (
          <marker key={type} id={`rd-${type}`} markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
            <path d="M0,0 L7,2.5 L0,5" fill={style.lineColor} />
          </marker>
        ))}
      </defs>

      {/* ══ Ingest arrows (↓ into channels) ══ */}
      {CHANNELS.map((_, i) => (
        <line key={`in-${i}`}
          x1={chCx(i)} y1={ingestTop} x2={chCx(i)} y2={chY - 3}
          stroke="#94a3b8" strokeWidth="1.2" markerEnd="url(#rd-gray)"
        />
      ))}

      {/* ══ Channel boxes ══ */}
      {resolved.map((ch, i) => {
        const x = chLeft + i * (chW + chGap);
        const cx = chCx(i);
        return (
          <g key={ch.id}>
            <rect
              x={x} y={chY} width={chW} height={chH} rx="8"
              fill={ch.fill} stroke={ch.stroke} strokeWidth="1.2"
            />
            <text x={cx} y={chY + 17} textAnchor="middle"
              fontSize="10" fontWeight="700" fill={ch.text}>{ch.label}</text>
            <text x={cx} y={chY + 29} textAnchor="middle"
              fontSize="7" fill={ch.text} opacity="0.7">{ch.sub}</text>
            <text x={cx} y={chY + 41} textAnchor="middle"
              fontSize="7.5" fontFamily="monospace" fill={ch.text} opacity="0.55">
              filter:{ch.filter}
            </text>

            {ch.isOverride && (
              <text x={x + chW - 7} y={chY + 11} textAnchor="middle"
                fontSize="7" fill={ch.text} opacity="0.5" fontWeight="600">{"\u25cf"}</text>
            )}
          </g>
        );
      })}

      {/* ══ Routing curves: channel → sink ══ */}
      {resolved.map((ch, i) => {
        const fromX = chCx(i);
        const fromY = chY + chH + 2;
        const toX = sinkCxMap.get(ch.uri) ?? fromX;
        const toY = sinkY - 3;
        const st = SINK_STYLE[ch.type];
        const midY = (fromY + toY) / 2;

        return (
          <path key={`route-${ch.id}`}
            d={`M${fromX},${fromY} C${fromX},${midY} ${toX},${midY} ${toX},${toY}`}
            fill="none" stroke={st.lineColor} strokeWidth="1.2"
            opacity="0.6" markerEnd={`url(#rd-${ch.type})`}
          />
        );
      })}

      {/* ══ Sink target boxes ══ */}
      {destinations.map((uri, i) => {
        const st = SINK_STYLE[sinkType(uri)];
        const x = snkLeft + i * (snkW + snkGap);
        const cx = snkCx(i);

        return (
          <g key={uri ?? "stdout"}>
            <rect
              x={x} y={sinkY} width={snkW} height={snkH} rx="10"
              fill={st.fill} stroke={st.stroke} strokeWidth="1.5"
            />
            <text x={cx} y={sinkY + (uri ? 22 : 32)} textAnchor="middle"
              fontSize="10" fontWeight="700" fill={st.text}>{st.label}</text>
            {uri && (
              <text x={cx} y={sinkY + 38} textAnchor="middle"
                fontSize="7" fontFamily="monospace" fill={st.text}>
                {shortUri(uri, snkW > 128 ? 36 : 26)}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

/* ── Page component ──────────────────────────────────────────── */

export function RecordingSection() {
  const { data, error, fetchData } = usePlatformData();

  return (
    <PlatformDataGuard data={data} error={error}>
      {(d) => {
        const recFields = filterFields(d.config_fields, ["recording"]);
        return (
          <ServicePageLayout
            title="Recording"
            description="Structured logging with four semantic channels, each with its own filter vocabulary and sink routing"
          >
            <HowItWorks storageKey="recording">
              <p className="mb-2 text-xs text-gray-500">
                This diagram shows configured routing. Each channel uses its own destination if set,
                marked with a dot, or inherits <code>recording.sink</code>. With neither set, it writes to <code>stdout</code>.
              </p>
              <RecordingDiagram fields={recFields} />
              <div className="space-y-2 text-xs text-gray-500">
                <p className="font-semibold text-gray-600">Channel filters</p>
                <ul className="list-disc space-y-1.5 pl-4">
                  <li>
                    <strong className="text-gray-600">Observability.</strong> <code>digest</code> (default) keeps
                    INFO, WARN and ERROR. <code>detailed</code> adds DEBUG. <code>full</code> also includes TRACE.
                  </li>
                  <li>
                    <strong className="text-gray-600">Audit.</strong> <code>mutations</code> (default) excludes
                    read-style actions such as get or list. <code>all</code> includes them.
                  </li>
                  <li>
                    <strong className="text-gray-600">Security.</strong> <code>critical</code>, <code>high</code>
                    {" "}and <code>medium</code> set the minimum severity. <code>all</code> (default) keeps all severities.
                  </li>
                  <li>
                    <strong className="text-gray-600">Operations.</strong> <code>lifecycle</code> (default) keeps
                    INFO, WARN and ERROR. <code>detailed</code> includes all levels.
                  </li>
                </ul>
                <p>
                  Each channel accepts <code>off</code>. When <code>RUST_LOG</code> is non-empty, the tracing
                  log filter takes over from the channel filters. Destination selection stays the same.
                </p>
              </div>

              <div className="mt-4 space-y-2 border-t border-gray-100 pt-4 text-xs text-gray-500">
                <p className="font-semibold text-gray-600">Destinations</p>
                <ul className="list-disc space-y-1.5 pl-4">
                  <li><code>file://</code> appends formatted tracing text to a local file.</li>
                  <li>
                    <code>otlp+http://</code> and <code>otlp+grpc://</code> send traces to a collector when this
                    binary is built with OpenTelemetry. Without that feature, output for those destinations goes
                    to <code>stdout</code>. With an Observability OTLP destination, <code>recording.logs_enabled</code>
                    {" "}can also export log events.
                  </li>
                </ul>
                <p>The recording pipeline is initialized at process startup. Restart AFP to apply changes to recording settings.</p>
              </div>
            </HowItWorks>

            <ConfigSection storageKey="recording" fields={recFields} onRefresh={fetchData} />
          </ServicePageLayout>
        );
      }}
    </PlatformDataGuard>
  );
}
