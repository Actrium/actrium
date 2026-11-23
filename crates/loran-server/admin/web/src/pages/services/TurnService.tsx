import { ServiceAccessCard } from "./ServiceAccessCard";
import { useEffect, useState, useCallback } from "react";
import { api, type ServiceDetail } from "../../lib/api";
import { ServicePageLayout, StatusSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import { ServiceMetrics } from "./shared";

function TurnDiagram({ config }: { config: Record<string, unknown> }) {
  const host = String(config.bind_ip ?? "?");
  const bindIp = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  const bindPort = String(config.bind_port ?? "?");
  const advIp = String(config.advertised_ip ?? "?");
  const advPort = String(config.advertised_port ?? "?");
  const relay = String(config.relay_port_range ?? "?");
  const realm = String(config.realm ?? "?");

  return (
    <svg
      viewBox="0 0 720 290"
      role="img"
      aria-label="Example TURN relay path showing advertised endpoints and local sockets"
      className="max-w-3xl mx-auto"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <marker id="arr-blue" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#3b82f6" />
        </marker>
        <marker id="arr-blue-rev" markerWidth="7" markerHeight="5" refX="0" refY="2.5" orient="auto">
          <path d="M7,0 L0,2.5 L7,5" fill="#3b82f6" />
        </marker>
        <marker id="arr-green" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#10b981" />
        </marker>
        <marker id="arr-green-rev" markerWidth="7" markerHeight="5" refX="0" refY="2.5" orient="auto">
          <path d="M7,0 L0,2.5 L7,5" fill="#10b981" />
        </marker>
      </defs>

      {/* Example: ICE selected a relayed candidate. */}
      <rect x="16" y="24" width="110" height="48" rx="10" fill="#dbeafe" stroke="#3b82f6" strokeWidth="1.5" />
      <text x="71" y="46" textAnchor="middle" fontSize="12" fontWeight="600" fill="#1e40af">Peer A</text>
      <text x="71" y="60" textAnchor="middle" fontSize="9" fill="#3b82f6">TURN client</text>

      <rect x="594" y="24" width="110" height="48" rx="10" fill="#d1fae5" stroke="#10b981" strokeWidth="1.5" />
      <text x="649" y="46" textAnchor="middle" fontSize="12" fontWeight="600" fill="#065f46">Peer B</text>
      <text x="649" y="60" textAnchor="middle" fontSize="9" fill="#10b981">Remote peer</text>

      <text x="360" y="50" textAnchor="middle" fontSize="11" fill="#475569" fontWeight="600">Example: ICE selects a TURN relay</text>

      {/* ══ Layer 2 — Advertised endpoints ══ */}
      <rect x="178" y="78" width="378" height="56" rx="10" fill="none" stroke="#d97706" strokeWidth="1.2" strokeDasharray="4 2" />
      <text x="367" y="92" textAnchor="middle" fontSize="9" fontWeight="600" fill="#92400e">Advertised IP: {advIp}</text>

      {/* Control port */}
      <rect x="190" y="98" width="160" height="28" rx="6" fill="#fef3c7" stroke="#d97706" strokeWidth="1" />
      <text x="270" y="116" textAnchor="middle" fontSize="10" fontWeight="600" fill="#92400e">Control :{advPort}</text>

      {/* Relay address */}
      <rect x="366" y="98" width="180" height="28" rx="6" fill="#fef3c7" stroke="#d97706" strokeWidth="1" />
      <text x="456" y="116" textAnchor="middle" fontSize="10" fontWeight="600" fill="#92400e">Relay: a port in {relay}</text>

      {/* Peer A ↔ Control */}
      <path d="M71,72 L71,112 L190,112" fill="none" stroke="#3b82f6" strokeWidth="1.5" markerStart="url(#arr-blue-rev)" markerEnd="url(#arr-blue)" />
      <text x="125" y="100" textAnchor="middle" fontSize="8" fill="#3b82f6" fontWeight="500">TURN protocol</text>

      {/* Peer B ↔ Relay */}
      <path d="M649,72 L649,112 L546,112" fill="none" stroke="#10b981" strokeWidth="1.5" markerStart="url(#arr-green-rev)" markerEnd="url(#arr-green)" />
      <text x="602" y="100" textAnchor="middle" fontSize="8" fill="#10b981" fontWeight="500">Raw UDP</text>

      {/* ══ Layer 3 — Routing to the local sockets ══ */}
      <rect x="186" y="146" width="360" height="20" rx="4" fill="#fbbf24" opacity="0.25" stroke="#d97706" strokeWidth="0.8" />
      <text x="366" y="160" textAnchor="middle" fontSize="9" fontWeight="600" fill="#92400e">UDP routing · port mapping if behind NAT</text>

      {/* Connect each advertised endpoint to its local socket. */}
      <line x1="271" y1="126" x2="271" y2="146" stroke="#d97706" strokeWidth="1" />
      <line x1="271" y1="166" x2="271" y2="182" stroke="#d97706" strokeWidth="1" />
      <line x1="458" y1="126" x2="458" y2="146" stroke="#d97706" strokeWidth="1" />
      <line x1="458" y1="166" x2="458" y2="182" stroke="#d97706" strokeWidth="1" />

      {/* ══ Layer 4 — TURN Process ══ */}
      <rect x="176" y="182" width="380" height="100" rx="12" fill="#f8fafc" stroke="#cbd5e1" strokeWidth="1.5" />

      <text x="186" y="202" fontSize="9" fontWeight="600" fill="#475569">TURN Process</text>
      <text x="186" y="214" fontSize="8" fill="#9ca3af">TURN auth realm: {realm}</text>

      {/* Bind listener */}
      <rect x="196" y="226" width="160" height="40" rx="8" fill="#e0e7ff" stroke="#6366f1" strokeWidth="1.2" />
      <text x="276" y="243" textAnchor="middle" fontSize="10" fontWeight="600" fill="#3730a3">UDP control listener</text>
      <text x="276" y="257" textAnchor="middle" fontSize="9" fontWeight="500" fill="#312e81">{bindIp}:{bindPort}</text>

      {/* Relay port sockets */}
      <rect x="376" y="226" width="168" height="40" rx="8" fill="#ecfdf5" stroke="#10b981" strokeWidth="1.2" />
      <text x="460" y="243" textAnchor="middle" fontSize="10" fontWeight="600" fill="#065f46">Relay sockets</text>
      <text x="460" y="257" textAnchor="middle" fontSize="9" fontWeight="500" fill="#047857">local UDP ports {relay}</text>

      {/* Internal relay arrow */}
      <line x1="356" y1="246" x2="376" y2="246" stroke="#9ca3af" strokeWidth="1" strokeDasharray="3 2" />
    </svg>
  );
}

export function TurnService() {
  const [data, setData] = useState<ServiceDetail | null>(null);
  const [error, setError] = useState("");

  const fetchData = useCallback(async () => {
    try {
      const d = await api.getServiceDetail("turn");
      setData(d);
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load");
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, [fetchData]);

  if (error && !data) {
    return (
      <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
        {error}
      </div>
    );
  }

  if (!data) {
    return <div className="text-sm text-gray-500">Loading...</div>;
  }

  return (
    <ServicePageLayout
      title="TURN Service"
      description="Traversal Using Relays around NAT — forwards WebRTC traffic through an ICE relay"
    >
      <StatusSection
        enabled={data.enabled}
        healthy={data.status?.is_healthy}
        disabledHint={<>This service is not enabled. Set the TURN bit (bit 2) in the <code>enable</code> bitmask to activate it.</>}
      />

      <ServiceAccessCard service="turn" data={data} onRefresh={fetchData} />

      {data.enabled && <ServiceMetrics status={data.status} storageKey="turn" />}

      {data.config && (
        <HowItWorks storageKey="turn">
          <p className="text-xs text-gray-500 mb-4">
            A TURN client authenticates on the UDP control endpoint and requests an allocation. A successful
            allocation returns a relay address in <code>XOR-RELAYED-ADDRESS</code>. When ICE selects that address,
            the client speaks TURN to the control listener and the remote peer exchanges UDP with the allocated relay socket.
          </p>
          <TurnDiagram config={data.config} />

          <div className="mt-5 space-y-2 text-xs text-gray-500 border-t border-gray-100 pt-4">
            <p className="font-semibold text-gray-600">Deployment notes</p>
            <ul className="list-disc pl-4 space-y-1.5">
              <li>
                <strong className="text-gray-600">Control port.</strong> Clients use the advertised UDP endpoint.
                The server listens on <code>bind.ice.ip</code> and <code>bind.ice.port</code>. The advertised control
                port can differ from the local one if UDP is forwarded between them.
              </li>
              <li>
                <strong className="text-gray-600">Relay ports.</strong> TURN binds local sockets from <code>turn.relay_port_range</code>
                and announces each allocation with <code>bind.ice.advertised_ip</code> and the allocated port.
                If the host is behind NAT, peers must be able to reach those UDP ports and receive traffic from the
                same advertised address and port. A directly routed host does not need NAT.
              </li>
              <li>
                <strong className="text-gray-600">Authentication.</strong> <code>turn.realm</code> names the TURN
                authentication realm. An Actr Realm identifies an App. Depending on configuration, TURN accepts
                Issuer credentials scoped to an Actr Realm, static users, or credentials from an external TURN REST issuer.
              </li>
              <li>
                <strong className="text-gray-600">Peer addresses.</strong> Normal deployments refuse relay traffic
                to loopback, private and link-local peer addresses.
              </li>
            </ul>
          </div>
        </HowItWorks>
      )}
    </ServicePageLayout>
  );
}
