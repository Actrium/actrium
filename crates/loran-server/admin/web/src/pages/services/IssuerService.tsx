import { useEffect, useState, useCallback } from "react";
import { api, type ServiceDetail } from "../../lib/api";
import { ServicePageLayout, ConfigSection, StatusSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import { ServiceMetrics } from "./shared";
import { DevelopmentJoinSettings } from "./DevelopmentJoinSettings";

function IssuerDiagram({ config }: { config: Record<string, unknown> }) {
  const ttl = String(config.token_ttl_secs ?? "3600");
  const hb = String(config.signaling_heartbeat_interval_secs ?? "30");

  const kX = 80;   // Signer (left — backend dependency)
  const aX = 280;  // Issuer (center)
  const cX = 480;  // Client (right — flows toward Signaling)
  const sX = 640;  // Signaling (far right, no lifeline — just destination)
  const headY = 8;
  const lifeTop = 56;
  const lifeBot = 280;

  return (
    <svg
      viewBox="0 0 720 292"
      className="max-w-3xl mx-auto"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <marker id="issuer-ab" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#3b82f6" />
        </marker>
        <marker id="issuer-ag" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#10b981" />
        </marker>
        <marker id="issuer-ap" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#8b5cf6" />
        </marker>
        <marker id="issuer-ao" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#d97706" />
        </marker>
      </defs>

      {/* Column heads */}
      <rect x={kX - 50} y={headY} width="100" height="40" rx="8" fill="#fef3c7" stroke="#d97706" strokeWidth="1.5" />
      <text x={kX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#92400e">Signer</text>
      <text x={kX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#d97706">Signing Oracle</text>

      <rect x={aX - 50} y={headY} width="100" height="40" rx="8" fill="#e0e7ff" stroke="#6366f1" strokeWidth="1.5" />
      <text x={aX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#3730a3">Issuer</text>
      <text x={aX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#6366f1">Identity Issuer</text>

      <rect x={cX - 50} y={headY} width="100" height="40" rx="8" fill="#dbeafe" stroke="#3b82f6" strokeWidth="1.5" />
      <text x={cX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#1e40af">Client</text>
      <text x={cX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#3b82f6">WebRTC peer</text>

      {/* Signaling — destination only (no lifeline) */}
      <rect x={sX - 50} y={headY} width="100" height="40" rx="8" fill="#d1fae5" stroke="#10b981" strokeWidth="1.5" strokeDasharray="4 2" />
      <text x={sX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#065f46">Signaling</text>
      <text x={sX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#10b981">WebSocket</text>

      {/* Lifelines */}
      <line x1={kX} y1={lifeTop} x2={kX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />
      <line x1={aX} y1={lifeTop} x2={aX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />
      <line x1={cX} y1={lifeTop} x2={cX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />
      <line x1={sX} y1={lifeTop} x2={sX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />

      {/* 1. Issuer gets the Realm's issuer key from Signer */}
      <line x1={aX - 4} y1={74} x2={kX + 4} y2={74} stroke="#d97706" strokeWidth="1.5" markerEnd="url(#issuer-ao)" />
      <text x={(kX + aX) / 2} y={68} textAnchor="middle" fontSize="9" fontWeight="600" fill="#d97706">EnsureNamedKey</text>

      <line x1={kX + 4} y1={94} x2={aX - 4} y2={94} stroke="#d97706" strokeWidth="1.5" markerEnd="url(#issuer-ao)" />
      <text x={(kX + aX) / 2} y={88} textAnchor="middle" fontSize="8" fill="#d97706">issuer:&lt;realm&gt; key</text>

      {/* Cache note */}
      <rect x={aX - 86} y={100} width="80" height="18" rx="3" fill="#fef3c7" stroke="#fcd34d" strokeWidth="0.8" />
      <text x={aX - 46} y={112} textAnchor="middle" fontSize="7" fill="#92400e">cache per Realm</text>

      {/* 2. Client registers */}
      <line x1={cX - 4} y1={136} x2={aX + 4} y2={136} stroke="#3b82f6" strokeWidth="1.5" markerEnd="url(#issuer-ab)" />
      <text x={(aX + cX) / 2} y={130} textAnchor="middle" fontSize="9" fontWeight="600" fill="#3b82f6">POST /register</text>

      {/* 3. Issuer issues credential */}
      <rect x={aX - 44} y={148} width="88" height="36" rx="4" fill="#f5f3ff" stroke="#8b5cf6" strokeWidth="0.8" />
      <text x={aX} y={160} textAnchor="middle" fontSize="7" fontWeight="600" fill="#6d28d9">Generate ActrId</text>
      <text x={aX} y={170} textAnchor="middle" fontSize="7" fill="#7c3aed">Ed25519 sign</text>
      <text x={aX} y={180} textAnchor="middle" fontSize="7" fill="#7c3aed">via Signer</text>

      {/* 4. Response to client */}
      <line x1={aX + 4} y1={198} x2={cX - 4} y2={198} stroke="#10b981" strokeWidth="1.5" markerEnd="url(#issuer-ag)" />
      <text x={(aX + cX) / 2} y={192} textAnchor="middle" fontSize="9" fontWeight="600" fill="#10b981">Credential</text>
      <text x={(aX + cX) / 2} y={210} textAnchor="middle" fontSize="8" fill="#6b7280">ActrId + access/TURN + renew token</text>

      {/* 5. Client connects to Signaling */}
      <line x1={cX + 4} y1={240} x2={sX - 4} y2={240} stroke="#10b981" strokeWidth="1.5" markerEnd="url(#issuer-ag)" />
      <text x={(cX + sX) / 2} y={234} textAnchor="middle" fontSize="9" fontWeight="600" fill="#10b981">Connect</text>
      <text x={(cX + sX) / 2} y={252} textAnchor="middle" fontSize="8" fill="#6b7280">with credential</text>

      {/* Heartbeat */}
      <line x1={cX + 4} y1={270} x2={sX - 4} y2={270} stroke="#10b981" strokeWidth="1" strokeDasharray="3 2" markerEnd="url(#issuer-ag)" />
      <text x={(cX + sX) / 2} y={282} textAnchor="middle" fontSize="7" fill="#10b981">heartbeat every {hb}s</text>

      {/* TTL annotation */}
      <rect x={kX - 50} y={230} width="100" height="30" rx="4" fill="#f8fafc" stroke="#e2e8f0" strokeWidth="0.8" />
      <text x={kX} y={243} textAnchor="middle" fontSize="7" fill="#64748b">token_ttl: {ttl}s</text>
      <text x={kX} y={253} textAnchor="middle" fontSize="7" fill="#64748b">heartbeat: {hb}s</text>
    </svg>
  );
}

export function IssuerService() {
  const [data, setData] = useState<ServiceDetail | null>(null);
  const [error, setError] = useState("");

  const fetchData = useCallback(async () => {
    try {
      setData(await api.getServiceDetail("issuer"));
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
      title="Issuer Service"
      description="Actor Identity Service — issues signed credentials for WebRTC actors"
    >
      <StatusSection
        enabled={data.enabled}
        healthy={data.status?.is_healthy}
        disabledHint={<>This service is not enabled. Set the Issuer bit (bit 3) in the <code>enable</code> bitmask to activate it.</>}
      />

      {data.enabled && <ServiceMetrics status={data.status} storageKey="issuer" />}
      {data.enabled && data.config_fields && (
        <DevelopmentJoinSettings fields={data.config_fields}
          writable={data.config?.dev_join_writable === true} onRefresh={fetchData} />
      )}

      {data.config && (
        <HowItWorks storageKey="issuer">
          <p className="text-xs text-gray-500 mb-4">
            Issuer issues identity credentials for WebRTC peers. Each Realm has its own issuer key,
            <code>issuer:&lt;realm&gt;</code>, held by Signer and listed in the Realm Profile. When a
            client registers, Issuer generates a unique ActrId, calls Signer to Ed25519-sign the
            credential claims with the Realm's key, and returns the signed access credential, TURN
            credential, and renewal token. Retiring a Realm retires its key. The client then uses the
            access credential to connect to the Signaling server.
          </p>
          <IssuerDiagram config={data.config} />

          <div className="mt-5 space-y-2 text-xs text-gray-500 border-t border-gray-100 pt-4">
            <p className="font-semibold text-gray-600">Key concepts</p>
            <ul className="list-disc pl-4 space-y-1.5">
              <li>
                <strong className="text-gray-600">token_ttl_secs</strong> controls how long the issued
                credential is valid. Clients must re-register after expiry.
              </li>
              <li>
                <strong className="text-gray-600">signaling_heartbeat_interval_secs</strong> is sent to
                the client so it knows how often to ping the Signaling server to maintain its WebSocket connection.
              </li>
              <li>
                Realm issuer keys do not expire. Signing needs Signer, so registration fails while
                Signer is unavailable.
              </li>
            </ul>
          </div>
        </HowItWorks>
      )}

      {data.config_fields && (
        <ConfigSection storageKey="issuer" fields={data.config_fields} onRefresh={fetchData} />
      )}
    </ServicePageLayout>
  );
}
