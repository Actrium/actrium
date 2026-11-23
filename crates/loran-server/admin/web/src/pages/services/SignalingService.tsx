import { ServiceAccessCard } from "./ServiceAccessCard";
import { useEffect, useState, useCallback } from "react";
import { api, type ServiceDetail } from "../../lib/api";
import { ServicePageLayout, StatusSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import { ServiceMetrics } from "./shared";

function SignalingDiagram() {
  /* ── layout ── */
  const aX = 80;   // Peer A
  const sX = 310;  // Signaling Server
  const bX = 540;  // Peer B
  const headY = 28;
  const lifeTop = 76;
  const lifeBot = 266;

  return (
    <svg
      viewBox="0 0 620 310"
      role="img"
      aria-label="Signaling sequence after both peers register with Issuer"
      className="max-w-2xl mx-auto"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <marker id="sig-ar" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#3b82f6" />
        </marker>
        <marker id="sig-ag" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#10b981" />
        </marker>
        <marker id="sig-ao" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#d97706" />
        </marker>
        <marker id="sig-ap" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#8b5cf6" />
        </marker>
      </defs>

      <text x={sX} y={15} textAnchor="middle" fontSize="10" fontWeight="600" fill="#475569">
        Both peers have registered with Issuer before connecting
      </text>

      {/* ══ Column heads ══════════════════════════════ */}
      <rect x={aX - 50} y={headY} width="100" height="40" rx="8" fill="#dbeafe" stroke="#3b82f6" strokeWidth="1.5" />
      <text x={aX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#1e40af">Peer A</text>
      <text x={aX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#3b82f6">caller</text>

      <rect x={sX - 60} y={headY} width="120" height="40" rx="8" fill="#e0e7ff" stroke="#6366f1" strokeWidth="1.5" />
      <text x={sX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#3730a3">Signaling</text>
      <text x={sX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#6366f1">/signaling/ws</text>

      <rect x={bX - 50} y={headY} width="100" height="40" rx="8" fill="#d1fae5" stroke="#10b981" strokeWidth="1.5" />
      <text x={bX} y={headY + 18} textAnchor="middle" fontSize="11" fontWeight="600" fill="#065f46">Peer B</text>
      <text x={bX} y={headY + 31} textAnchor="middle" fontSize="8" fill="#10b981">callee</text>

      {/* ══ Lifelines ═════════════════════════════════ */}
      <line x1={aX} y1={lifeTop} x2={aX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />
      <line x1={sX} y1={lifeTop} x2={sX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />
      <line x1={bX} y1={lifeTop} x2={bX} y2={lifeBot} stroke="#cbd5e1" strokeWidth="1" strokeDasharray="4 3" />

      {/* ── 1. Authenticate on upgrade ──────────────── */}
      <line x1={aX + 4} y1={96} x2={sX - 4} y2={96} stroke="#3b82f6" strokeWidth="1.5" markerEnd="url(#sig-ar)" />
      <text x={(aX + sX) / 2} y={90} textAnchor="middle" fontSize="9" fontWeight="600" fill="#3b82f6">WS upgrade with credential</text>

      <line x1={bX - 4} y1={96} x2={sX + 4} y2={96} stroke="#10b981" strokeWidth="1.5" markerEnd="url(#sig-ag)" />
      <text x={(sX + bX) / 2} y={90} textAnchor="middle" fontSize="9" fontWeight="600" fill="#047857">WS upgrade with credential</text>

      {/* ── 2. Find an eligible Actor for a known type ─ */}
      <line x1={aX + 4} y1={120} x2={sX - 4} y2={120} stroke="#d97706" strokeWidth="1.5" markerEnd="url(#sig-ao)" />
      <text x={(aX + sX) / 2} y={114} textAnchor="middle" fontSize="9" fontWeight="600" fill="#92400e">RouteCandidates (service type)</text>

      <line x1={sX - 4} y1={140} x2={aX + 4} y2={140} stroke="#d97706" strokeWidth="1.5" markerEnd="url(#sig-ao)" />
      <text x={(aX + sX) / 2} y={134} textAnchor="middle" fontSize="8" fill="#92400e">Eligible Actor IDs</text>

      {/* ── 3. SDP Offer/Answer ────────────────────── */}
      <line x1={aX + 4} y1={162} x2={sX - 4} y2={162} stroke="#8b5cf6" strokeWidth="1.5" markerEnd="url(#sig-ap)" />
      <line x1={sX + 4} y1={162} x2={bX - 4} y2={162} stroke="#8b5cf6" strokeWidth="1.5" markerEnd="url(#sig-ap)" />
      <text x={sX} y={156} textAnchor="middle" fontSize="9" fontWeight="600" fill="#8b5cf6">SDP Offer</text>

      <line x1={bX - 4} y1={186} x2={sX + 4} y2={186} stroke="#8b5cf6" strokeWidth="1.5" markerEnd="url(#sig-ap)" />
      <line x1={sX - 4} y1={186} x2={aX + 4} y2={186} stroke="#8b5cf6" strokeWidth="1.5" markerEnd="url(#sig-ap)" />
      <text x={sX} y={180} textAnchor="middle" fontSize="9" fontWeight="600" fill="#8b5cf6">SDP Answer</text>

      {/* ── 4. ICE candidates ─────────────────────── */}
      <line x1={aX + 4} y1={212} x2={sX - 4} y2={212} stroke="#8b5cf6" strokeWidth="1.2" strokeDasharray="5 3" markerEnd="url(#sig-ap)" />
      <line x1={sX + 4} y1={212} x2={bX - 4} y2={212} stroke="#8b5cf6" strokeWidth="1.2" strokeDasharray="5 3" markerEnd="url(#sig-ap)" />
      <text x={sX} y={206} textAnchor="middle" fontSize="8" fill="#8b5cf6">ICE candidates</text>

      <line x1={bX - 4} y1={230} x2={sX + 4} y2={230} stroke="#8b5cf6" strokeWidth="1.2" strokeDasharray="5 3" markerEnd="url(#sig-ap)" />
      <line x1={sX - 4} y1={230} x2={aX + 4} y2={230} stroke="#8b5cf6" strokeWidth="1.2" strokeDasharray="5 3" markerEnd="url(#sig-ap)" />
      <text x={sX} y={224} textAnchor="middle" fontSize="8" fill="#8b5cf6">ICE candidates</text>

      <rect x={sX - 146} y={275} width="292" height="24" rx="5" fill="#ecfdf5" />
      <text x={sX} y={290} textAnchor="middle" fontSize="10" fontWeight="600" fill="#065f46">
        Peers run ICE checks for a direct or TURN path
      </text>
    </svg>
  );
}

export function SignalingService() {
  const [data, setData] = useState<ServiceDetail | null>(null);
  const [error, setError] = useState("");

  const fetchData = useCallback(async () => {
    try {
      const d = await api.getServiceDetail("signaling");
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
      title="Signaling Service"
      description="WebSocket signaling server for WebRTC session negotiation"
    >
      <StatusSection
        enabled={data.enabled}
        healthy={data.status?.is_healthy}
        disabledHint={<>This service is not enabled. Set the Signaling bit (bit 0) in the <code>enable</code> bitmask to activate it.</>}
      />

      <ServiceAccessCard service="signaling" data={data} onRefresh={fetchData} />

      {data.enabled && <ServiceMetrics status={data.status} storageKey="signaling" />}

      {data.config && (
        <HowItWorks storageKey="signaling">
          <p className="text-xs text-gray-500 mb-4">
            Each peer first registers over HTTP(S) at <code>/issuer/register</code>. Signaling verifies the Actor
            credential and the active identity binding before upgrading <code>/signaling/ws</code> to a WebSocket.
            It then registers the live connection for discovery and routing.
          </p>
          <SignalingDiagram />
          <div className="mt-4 space-y-2 border-t border-gray-100 pt-4 text-xs text-gray-500">
            <p>
              <strong className="text-gray-600">Discovery</strong> lists service types the caller may access in
              its Realm. <strong className="text-gray-600">RouteCandidates</strong> finds and ranks eligible online Actors
              of a requested type. Signaling forwards authorized role negotiation, SDP and ICE messages between the peers.
            </p>
            <p>
              WebRTC traffic uses the direct or TURN relay path selected by ICE. WSS Relay is a separate transport
              for application data over WebSocket.
            </p>
          </div>
        </HowItWorks>
      )}
    </ServicePageLayout>
  );
}
