import { useEffect, useRef, useState } from "react";
import {
  RefreshCw,
  Loader2,
  Server,
  Shield,
  ArrowLeftRight,
  Radio,
  Fingerprint,
  Key,
  Check,
} from "lucide-react";
import { api, type ServiceStatus } from "../../lib/api";
import { ServicePageLayout } from "../../components/layout/ServicePageLayout";
import { CollapsibleCard } from "../../components/ui/CollapsibleCard";
import { usePlatformData } from "../platform/shared";

const formatHostPort = (host: string, port: number | string): string => {
  if (!host) return "";
  const needsBrackets = host.includes(":") && !host.startsWith("[");
  const h = needsBrackets ? `[${host}]` : host;
  return `${h}:${port}`;
};

/** Copy text to clipboard — works on HTTP (non-secure) contexts too */
function copyText(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    return navigator.clipboard.writeText(text).catch(() => fallbackCopy(text));
  }
  return fallbackCopy(text);
}

function fallbackCopy(text: string): Promise<void> {
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  document.body.removeChild(ta);
  return Promise.resolve();
}

/** Inline click-to-copy code snippet — blends into prose */
function C({ c }: { c: string }) {
  const [copied, setCopied] = useState(false);
  const copy = (e: React.MouseEvent) => {
    e.stopPropagation();
    copyText(c).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <code onClick={copy} className="inline cursor-pointer rounded bg-gray-100 px-1 py-px text-[11px] font-mono text-gray-700 hover:bg-gray-200 transition-colors" title="Click to copy">
      {c}{copied && <Check className="inline ml-0.5 h-2.5 w-2.5 text-green-600" />}
    </code>
  );
}

type TestStatus = "idle" | "testing" | "success" | "failure";

interface ServiceTestResult {
  status: TestStatus;
  message?: string;
  latency?: number;
}

const SERVICE_ICONS: Record<string, any> = {
  "stun": Shield,
  "turn": ArrowLeftRight,
  "signaling": Radio,
  "issuer": Fingerprint,
  "signer": Key,
};

export function NetworkPage() {
  const [services, setServices] = useState<ServiceStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, ServiceTestResult>>({});
  const { data: platformData } = usePlatformData();

  const fetchServices = async () => {
    try {
      setLoading(true);
      const data = await api.getServices();
      setServices(data.services);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load services");
    } finally {
      setLoading(false);
    }
  };

  const inFlight = useRef(new Set<string>());
  useEffect(() => { void fetchServices(); }, []);

  const updateTestResult = (name: string, result: ServiceTestResult) => {
    setTestResults(prev => ({ ...prev, [name]: result }));
  };
  

  const testHttpService = async (service: ServiceStatus) => {
    const name = service.name;
    updateTestResult(name, { status: "testing" });

    try {
      if (!service.port) throw Error("No port reported");
      const data = await api.probePort(service.port, service.type);
      if (data.reachable) {
        updateTestResult(name, { status: "success", message: "Node connected to "+data.target, latency: data.latency_ms });
      } else {
        updateTestResult(name, { status: "failure", message: data.error || "Unreachable" });
      }
    } catch (err) {
      updateTestResult(name, {
        status: "failure",
        message: err instanceof Error ? err.message : "Connection failed"
      });
    }
  };

  // Test UDP reachability via STUN binding request (shared by STUN & TURN on same port)

  const testStunTurnService = async (service: ServiceStatus) => {
    const advertised = platformData?.config_fields.find(f => f.key === "bind.ice.advertised_port")?.effective_value;
    const port = Number(advertised) || service.port;
    const host = platformData?.config_fields.find(f => f.key === "bind.ice.advertised_ip")?.effective_value;
    if (!port || !host) { updateTestResult(service.name,{status:"failure",message:"Advertised ICE address unavailable"}); return; }

    // Only test each UDP port once — apply result to all services on that port
    const probe = formatHostPort(host,port);
    if (inFlight.current.has(probe)) return;
    inFlight.current.add(probe);

    const samePort = services.filter(s => s.port === service.port && (s.type === 1 || s.type === 2));
    samePort.forEach(s => updateTestResult(s.name, { status: "testing" }));
    const startTime = performance.now();

    let pc: RTCPeerConnection | undefined;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      pc = new RTCPeerConnection({iceServers:[{urls:"stun:"+probe}]});
      const peer = pc;
      await new Promise<void>((resolve,reject) => {
        timeout = setTimeout(() => reject(Error("Browser did not obtain a STUN reflexive candidate")), 7000);
        peer.onicecandidate = ({candidate}) => {
          if (candidate?.type === "srflx") resolve();
          if (!candidate) reject(Error("ICE gathering ended without a reflexive candidate"));
        };
        peer.createDataChannel("probe");
        peer.createOffer().then(offer => peer.setLocalDescription(offer)).catch(reject);
      });
      samePort.forEach(s => updateTestResult(s.name, {status:"success", message:"Browser STUN binding only",latency:Math.round(performance.now()-startTime)}));
    } catch (err) {
      samePort.forEach(s => updateTestResult(s.name,{status:"failure",message:err instanceof Error ? err.message : "ICE check failed"}));
    } finally { clearTimeout(timeout); pc?.close(); inFlight.current.delete(probe); }
  };

  const runTest = (service: ServiceStatus) => {
    if (service.type === 1 || service.type === 2) {
        testStunTurnService(service);
    } else {
        testHttpService(service);
    }
  };


  const runAllTests = () => {

    services.forEach(runTest);
  };

  const started = useRef(false);
  useEffect(() => {
    if (started.current || !platformData || !services.length) return;
    started.current = true;
    runAllTests();
  }, [platformData, services]);

  if (loading) return <div className="flex h-full items-center justify-center"><Loader2 className="h-8 w-8 animate-spin text-blue-600" /></div>;
  if (error) return <div className="p-8 text-center text-red-600">{error}</div>;

  const configPort = (key: string) => Number(platformData?.config_fields.find(f => f.key === key)?.effective_value) || null;
  const httpPort = configPort("bind.http.port");
  const icePort = configPort("bind.ice.port");
  type ExtendedService = ServiceStatus & { enabled: boolean };
  const known = [
    {name:"Signer Service",type:5,port:null}, {name:"Issuer Service",type:4,port:httpPort},
    {name:"Signaling Service",type:3,port:httpPort}, {name:"WSS Relay",type:7,port:httpPort},
    {name:"STUN Server",type:1,port:icePort}, {name:"TURN Server",type:2,port:icePort},
  ];
  const allServices: ExtendedService[] = known.map(def => {
    const actual = services.find(s => s.type === def.type);
    return actual ? {...actual, enabled:true} : {...def,enabled:false,is_healthy:false,
      active_connections:0,total_requests:0,failed_requests:0,average_latency_ms:0,url:null,domain:null};
  });
  const portGroups: Record<string, ExtendedService[]> = {};
  for (const service of allServices) {
    if (!service.port) continue;
    const protocol = service.type === 1 || service.type === 2 ? "udp" : "tcp";
    const key = service.port + "/" + protocol;
    (portGroups[key] ||= []).push(service);
  }

  // ── Diagram layout ──────────────────────────────────────────
  const groupKeys = Object.keys(portGroups).sort((a, b) => parseInt(a) - parseInt(b) || a.localeCompare(b));
  const rowCount = groupKeys.length;

  // Pill zone (left)
  const PILL_W = 78, PILL_H = 42, PILL_GAP = 10;
  // Port-bar zone (center)
  const BAR_H = 26;
  const FW_X = 510;                            // center line of firewall/boundary
  const BOUNDARY_OFFSET = 10;                  // horizontal offset for entire boundary
  const effectiveFW_X = FW_X + BOUNDARY_OFFSET; // actual center after offset
  const BAR_INT_LEFT = 390;                    // internal bar left edge
  const BAR_EXT_RIGHT = 770;                   // external bar right edge
  // Rows
  const ROW_H = 80;
  const FIRST_ROW_CY = 60;
  const SVG_W = 900;
  const FW_WALL_TOP = 12;
  const SVG_H = FIRST_ROW_CY + Math.max(0, rowCount - 1) * ROW_H + 64;
  const rowCY = (i: number) => FIRST_ROW_CY + i * ROW_H;
  const LABEL_X = effectiveFW_X + 50;      // right-side text just outside the frame

  // Short pill labels — strip verbose suffixes
  const shortLabel = (name: string): string => {
    if (name === "WebSocket Relay Service") return "WSS Relay";
    return name.replace(/\s+(Service|Server)$/i, "");
  };

  // Colors
  const CG = "#2d6a2e";                        // dark green (success)
  const CR = "#a63028";                        // muted red  (failure)
  const CB = "#2563eb";                        // blue       (testing)
  const CN = "#94a3b8";                        // neutral gray (idle/untested)

  const NAME_ORDER: Record<string, number> = { "Signer": 0, "Signer Service": 0, "Issuer": 1, "Issuer Service": 1, "Signaling": 2, "Signaling Service": 2, "STUN": 3, "STUN Server": 3, "TURN": 4, "TURN Server": 4 };
  const sortedServices = [...allServices].sort((a, b) => (NAME_ORDER[a.name] ?? 99) - (NAME_ORDER[b.name] ?? 99));

  // Pill: service health — disabled → gray, healthy → green, unhealthy → red
  const pillFill = (s: ExtendedService) => {
    if (!s.enabled) return CN;
    return s.is_healthy ? CG : CR;
  };

  // Internal bar: port is listening → green; all disabled → gray
  const intBarFill = (svcs: ServiceStatus[]) => {
    if (svcs.some(s => s.is_healthy)) return CG;
    return CN;
  };

  // External bar: browser test result
  const extBarFill = (svcs: ServiceStatus[]) => {
    if (svcs.some(s => testResults[s.name]?.status === "failure")) return CR;
    if (svcs.some(s => testResults[s.name]?.status === "testing")) return CB;
    if (svcs.every(s => testResults[s.name]?.status === "success")) return CG;
    return CN;
  };

  // Labels beside ports
  const advIp = platformData?.config_fields.find(f => f.key === "bind.ice.advertised_ip")?.effective_value ?? "";
  const advPort = platformData?.config_fields.find(f => f.key === "bind.ice.advertised_port")?.effective_value ?? "";
  const relayRange = platformData?.config_fields.find(f => f.key === "turn.relay_port_range")?.effective_value || "—";

  const isTesting = (svcs: ExtendedService[]) =>
    svcs.some(s => testResults[s.name]?.status === "testing");
  const svcTesting = (s: ExtendedService) =>
    testResults[s.name]?.status === "testing";

  return (
    <ServicePageLayout
      title="Network Diagnostics"
      description="Configured listeners and reachability checks. TCP runs on this node. Browser ICE checks STUN binding only. TURN allocation and WSS channel authentication require their own clients."
      headerActions={
        <button onClick={runAllTests} className="inline-flex items-center rounded-md bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm border border-gray-300 hover:bg-gray-50 active:scale-95 active:bg-gray-100 transition-transform duration-75">
          <RefreshCw className="mr-2 h-4 w-4" /> Run checks
        </button>
      }
    >

      <div className="flex flex-col items-center overflow-x-auto">
        <svg viewBox={`0 -40 ${SVG_W} ${SVG_H + 40}`} className="max-w-3xl mx-auto select-none">
          {/* ── Network boundary zone ── */}
          {(() => {
            const zoneW = 70;
            const zoneH = SVG_H - 36;
            const zoneX = effectiveFW_X - zoneW / 2;
            const outerY = FW_WALL_TOP;
            const backExpand = 10;
            const cellH = zoneH / 3;
            const foW = 44; // foreignObject width for composition
            const foH = 34; // foreignObject height
            const foX = effectiveFW_X - foW / 2;
            const cellCY = (i: number) => FW_WALL_TOP + cellH * i + cellH / 2;

            return (
              <>
                {/* Subtle layered back plate for depth */}
                <rect
                  x={zoneX - backExpand / 2}
                  y={outerY - backExpand / 2}
                  width={zoneW + backExpand}
                  height={zoneH + backExpand}
                  rx={7}
                  fill="#eff3fb"
                  stroke="#d5ddf0"
                  strokeWidth="1"
                  opacity="0.9"
                />
                {/* Single flat frame with subtle fill */}
                <rect
                  x={zoneX}
                  y={outerY}
                  width={zoneW}
                  height={zoneH}
                  rx={6}
                  fill="#f8fafc"
                  stroke="#cbd5e1"
                  strokeWidth="1.1"
                />
                {/* Label just above the frame */}
                <text
                  x={effectiveFW_X}
                  y={FW_WALL_TOP - 11}
                  textAnchor="middle"
                  fontSize="10"
                  fontWeight="600"
                  fill="#94a3b8"
                  letterSpacing="0.08em"
                >
                  NETWORK BOUNDARY
                </text>
                {/* NAT */}
                <foreignObject x={foX} y={cellCY(0) - foH / 2} width={foW} height={foH}>
                  <div className="flex items-center justify-center h-full">
                    <ArrowLeftRight className="h-6 w-6 text-slate-400" />
                  </div>
                </foreignObject>
                {/* Firewall */}
                <foreignObject x={foX} y={cellCY(1) - foH / 2} width={foW} height={foH}>
                  <div className="flex items-center justify-center h-full">
                    <Shield className="h-6 w-6 text-slate-400" />
                  </div>
                </foreignObject>
                {/* Firewall */}
                <foreignObject x={foX} y={cellCY(2) - foH / 2} width={foW} height={foH}>
                  <div className="flex items-center justify-center h-full">
                    <Shield className="h-6 w-6 text-slate-400" />
                  </div>
                </foreignObject>
              </>
            );
          })()}


          {/* ── Port-group rows ── */}
          {groupKeys.map((port, idx) => {
            const svcs = [...portGroups[port]].sort((a, b) => (NAME_ORDER[a.name] ?? 99) - (NAME_ORDER[b.name] ?? 99));
            const udp = port.endsWith("/udp");
            const relayPorts = svcs.some(s => s.enabled && s.type === 2) && relayRange !== "—";
            const endpoints = [...new Set(svcs.flatMap(s => {
              if (!s.url || udp) return [];
              try { return [new URL(s.url).host]; } catch { return []; }
            }))];
            const cy = rowCY(idx);
            const iFill = intBarFill(svcs);
            const eFill = extBarFill(svcs);
            const testing = isTesting(svcs);
            const extClass = testing ? "testing" : undefined;

            return (
              <g key={port}>
                {/* Service pills */}
                {svcs.map((s, si) => {
                  const pillRight = BAR_INT_LEFT - 16;
                  const px = pillRight - (svcs.length - si) * (PILL_W + PILL_GAP) + PILL_GAP;
                  const py = cy - PILL_H / 2;
                  const color = pillFill(s);
                  const Icon = SERVICE_ICONS[s.name.toLowerCase()] || Server;

                  const showReason = true;

                  return (
                    <g key={s.name}>
                      <rect x={px} y={py} width={PILL_W} height={PILL_H} rx="14" fill="none" stroke={color} strokeWidth="1" className={svcTesting(s) ? "testing" : undefined} />
                      <foreignObject x={px} y={py} width={PILL_W} height={PILL_H}>
                        <div className="flex flex-col items-center justify-center h-full px-2" style={{ gap: showReason ? "1px" : "0" }}>
                          <div className="flex items-center gap-1.5">
                            <Icon className="h-3.5 w-3.5 shrink-0" strokeWidth={2} style={{ color }} />
                            <span className="text-[11px] font-semibold truncate" style={{ color }}>{shortLabel(s.name)}</span>
                          </div>
                          {showReason && (
                            <span className="text-[8px] truncate max-w-full opacity-80" style={{ color }}>{!s.enabled ? "Disabled" : s.is_healthy ? "Healthy" : "Unhealthy"}</span>
                          )}
                        </div>
                      </foreignObject>
                    </g>
                  );
                })}

                {/* Bars and port labels */}
                {udp ? (
                  <>
                    {/* ICE listener */}
                    <rect x={BAR_INT_LEFT} y={cy - 10} width={effectiveFW_X - BAR_INT_LEFT - 35} height={2} fill={iFill} />
                    <rect x={effectiveFW_X + 35} y={cy - 10} width={BAR_EXT_RIGHT - effectiveFW_X - 35} height={2} fill={eFill} className={extClass} />
                    <text x={effectiveFW_X - 50} y={cy - 14} textAnchor="end" fontSize="10" fontWeight="600" fontFamily="monospace" fill="#64748b">{port}</text>
                    {advIp && advPort && (
                      <text x={LABEL_X} y={cy - 14} textAnchor="start" fontSize="10" fontWeight="600" fill="#334155">
                        {formatHostPort(advIp, advPort)}
                      </text>
                    )}
                    {relayPorts && (
                      <text x={LABEL_X} y={cy + 21} textAnchor="start" fontSize="10" fontWeight="600" fill="#334155">
                        {relayRange}
                      </text>
                    )}

                    {relayPorts && <>
                    <rect x={BAR_INT_LEFT} y={cy - 5} width={effectiveFW_X - BAR_INT_LEFT - 35} height={16} fill={iFill} />
                    <rect x={effectiveFW_X + 35} y={cy - 5} width={BAR_EXT_RIGHT - effectiveFW_X - 35} height={16} fill={eFill} className={extClass} />
                    <text x={effectiveFW_X - 50} y={cy + 21} textAnchor="end" fontSize="10" fontWeight="600" fontFamily="monospace" fill="#64748b">{relayRange+"/udp"}</text></>}
                  </>
                ) : (
                  <>
                    {(() => { const h = BAR_H; return (
                      <>
                        <rect x={BAR_INT_LEFT} y={cy - h / 2} width={effectiveFW_X - BAR_INT_LEFT - 35} height={h} fill={iFill} />

                        <rect x={effectiveFW_X + 35} y={cy - h / 2} width={BAR_EXT_RIGHT - effectiveFW_X - 35} height={h} fill={eFill} className={extClass} />
                      </>
                    ); })()}
                    <text x={effectiveFW_X - 50} y={cy + 4} textAnchor="end" fontSize="10" fontWeight="600" fontFamily="monospace" fill="white">
                      {port}
                    </text>
                    {endpoints.length > 0 && (
                      <text x={LABEL_X} y={cy - 5} textAnchor="start" fontSize="10" fontWeight="600" fill="#334155">
                        <title>{endpoints.join(", ")}</title>{endpoints.length === 1 ? endpoints[0] : endpoints.length+" service endpoints"}
                      </text>
                    )}
                  </>
                )}
              </g>
            );
          })}
        </svg>

      </div>

      {/* ── Service Health ── */}
      {(() => {
        const hasIssue = sortedServices.some(s => s.enabled && !s.is_healthy);
        return (
        <CollapsibleCard storageKey="net_health" title="Service Health">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs font-mono text-gray-400">
                <th className="py-1.5 pr-4 font-normal w-28">Service</th>
                <th className="py-1.5 pr-4 font-normal w-24">Port</th>
                <th className="py-1.5 pr-4 font-normal w-20">Status</th>
                {hasIssue && <th className="py-1.5 font-normal">Troubleshoot</th>}
              </tr>
            </thead>
            <tbody>
              {sortedServices.map(s => {
                const proto = s.type === 1 || s.type === 2 ? "UDP" : "TCP";
                const label = shortLabel(s.name);
                return (
                  <tr key={s.name} className="border-b border-gray-100 last:border-0 align-top">
                    <td className="py-2 pr-4 font-medium text-gray-900">{label}</td>
                    <td className="py-2 pr-4 font-mono text-gray-600">
                      {s.port ? s.port + "/" + proto.toLowerCase() : "Unavailable"}
                      {s.type === 2 && relayRange !== "—" && <>, {relayRange}/udp</>}
                    </td>
                    <td className="py-2 pr-4">
                      {!s.enabled ? (
                        <span className="text-gray-400 text-xs font-medium">Disabled</span>
                      ) : s.is_healthy ? (
                        <span className="text-green-700 text-xs font-medium">Healthy</span>
                      ) : (
                        <span className="text-red-700 text-xs font-medium">Unhealthy</span>
                      )}
                    </td>
                    {hasIssue && (
                      <td className="py-2 text-xs text-gray-500 leading-relaxed">
                        {s.enabled && !s.is_healthy && (<>
                          {label} failed to initialize. Check logs
                          with <C c="journalctl -u loran -n 50 --no-pager" /> and
                          verify port binding via <C c={`sudo ss -${proto === 'UDP' ? 'u' : 't'}lnp sport = :${s.port}`} />.
                          Review <C c="cat /etc/loran/config.toml" /> for config errors.
                        </>)}
                      </td>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </CollapsibleCard>
        );
      })()}

      {/* ── Firewall / External Reachability ── */}
      {(() => {
        const fwGroups = Object.keys(portGroups).sort((a, b) => parseInt(a) - parseInt(b) || a.localeCompare(b));
        const hasFirewallIssue = fwGroups.some(port =>
          portGroups[port].some(s => testResults[s.name]?.status === "failure")
        );
        return (
        <CollapsibleCard storageKey="net_firewall" title="Reachability checks">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-200 text-left text-xs font-mono text-gray-400">
                <th className="py-1.5 pr-4 font-normal w-28">Port</th>
                <th className="py-1.5 pr-4 font-normal w-28">Services</th>
                <th className="py-1.5 pr-4 font-normal w-20">Status</th>
                {hasFirewallIssue && <th className="py-1.5 font-normal">Troubleshoot</th>}
              </tr>
            </thead>
            <tbody>
              {fwGroups.map(port => {
                const svcs = portGroups[port];

                const names = svcs.map(s => shortLabel(s.name)).join(", ");
                const isTesting = svcs.some(s => testResults[s.name]?.status === "testing");
                const isFail = svcs.some(s => testResults[s.name]?.status === "failure");
                const isSuccess = svcs.every(s => testResults[s.name]?.status === "success");
                const isIce = svcs[0].type === 1 || svcs[0].type === 2;

                return (
                  <tr key={port} className="border-b border-gray-100 last:border-0 align-top">
                    <td className="py-2 pr-4 font-mono text-gray-600">{port}</td>
                    <td className="py-2 pr-4 text-gray-900">{names}</td>
                    <td className="py-2 pr-4">
                      {isTesting ? (
                        <span className="flex items-center gap-1 text-blue-600 text-xs"><Loader2 className="h-3 w-3 animate-spin" />Testing</span>
                      ) : isSuccess ? (
                        <span className="text-green-700 text-xs font-medium">{isIce ? "STUN binding confirmed" : "TCP connected"}</span>
                      ) : isFail ? (
                        <span className="text-red-700 text-xs font-medium">Check unsuccessful</span>
                      ) : (
                        <span className="text-gray-400 text-xs">—</span>
                      )}
                    </td>
                    {hasFirewallIssue && (
                      <td className="py-2 text-xs text-gray-500 leading-relaxed">
                        {isFail && <span>{svcs.map(s => testResults[s.name]?.message).filter(Boolean)[0] || "Check failed"}</span>}
                      </td>
                    )}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </CollapsibleCard>
        );
      })()}
    </ServicePageLayout>
  );
}
