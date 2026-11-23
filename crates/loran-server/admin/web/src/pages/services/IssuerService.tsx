import { ServiceAccessCard } from "./ServiceAccessCard";
import { useEffect, useState, useCallback } from "react";
import { api, type ServiceDetail } from "../../lib/api";
import { ServicePageLayout, StatusSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import { ServiceMetrics } from "./shared";

function IssuerDiagram() {
  const kX = 80;
  const aX = 276;
  const cX = 486;
  const sX = 648;
  const lifeTop = 58;
  const lifeBot = 274;

  return (
    <svg
      viewBox="0 0 720 288"
      role="img"
      aria-label="Initial Actor registration, signing and Signaling connection"
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
        <marker id="issuer-ao" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#d97706" />
        </marker>
      </defs>

      <rect x={kX - 50} y="8" width="100" height="40" rx="8" fill="#fef3c7" stroke="#d97706" strokeWidth="1.5" />
      <text x={kX} y="32" textAnchor="middle" fontSize="11" fontWeight="600" fill="#92400e">Signer</text>
      <rect x={aX - 50} y="8" width="100" height="40" rx="8" fill="#e0e7ff" stroke="#6366f1" strokeWidth="1.5" />
      <text x={aX} y="32" textAnchor="middle" fontSize="11" fontWeight="600" fill="#3730a3">Issuer</text>
      <rect x={cX - 50} y="8" width="100" height="40" rx="8" fill="#dbeafe" stroke="#3b82f6" strokeWidth="1.5" />
      <text x={cX} y="32" textAnchor="middle" fontSize="11" fontWeight="600" fill="#1e40af">Client</text>
      <rect x={sX - 50} y="8" width="100" height="40" rx="8" fill="#d1fae5" stroke="#10b981" strokeWidth="1.5" />
      <text x={sX} y="32" textAnchor="middle" fontSize="11" fontWeight="600" fill="#065f46">Signaling</text>

      {[kX, aX, cX, sX].map((x) => (
        <line key={x} x1={x} y1={lifeTop} x2={x} y2={lifeBot} stroke="#cbd5e1" strokeDasharray="4 3" />
      ))}

      <line x1={cX - 4} y1="83" x2={aX + 4} y2="83" stroke="#3b82f6" strokeWidth="1.5" markerEnd="url(#issuer-ab)" />
      <text x={(aX + cX) / 2} y="77" textAnchor="middle" fontSize="10" fontWeight="600" fill="#1e40af">POST /issuer/register</text>
      <text x={(aX + cX) / 2} y="96" textAnchor="middle" fontSize="8" fill="#475569">Signed client proof</text>

      <rect x={aX - 79} y="104" width="158" height="32" rx="5" fill="#f5f3ff" stroke="#c4b5fd" />
      <text x={aX} y="117" textAnchor="middle" fontSize="9" fontWeight="600" fill="#6d28d9">Check membership and workload</text>
      <text x={aX} y="129" textAnchor="middle" fontSize="8" fill="#6d28d9">Reserve ActrId</text>

      <line x1={aX - 4} y1="162" x2={kX + 4} y2="162" stroke="#d97706" strokeWidth="1.5" markerEnd="url(#issuer-ao)" />
      <text x={(kX + aX) / 2} y="156" textAnchor="middle" fontSize="9" fontWeight="600" fill="#92400e">Sign with current Realm issuer key</text>
      <line x1={kX + 4} y1="186" x2={aX - 4} y2="186" stroke="#d97706" strokeWidth="1.5" markerEnd="url(#issuer-ao)" />
      <text x={(kX + aX) / 2} y="180" textAnchor="middle" fontSize="9" fill="#92400e">Ed25519 signature</text>

      <line x1={aX + 4} y1="219" x2={cX - 4} y2="219" stroke="#10b981" strokeWidth="1.5" markerEnd="url(#issuer-ag)" />
      <text x={(aX + cX) / 2} y="213" textAnchor="middle" fontSize="9" fontWeight="600" fill="#065f46">Registration response</text>
      <text x={(aX + cX) / 2} y="232" textAnchor="middle" fontSize="8" fill="#475569">ActrId, Actor/TURN credentials, renewal token</text>

      <line x1={cX + 4} y1="258" x2={sX - 4} y2="258" stroke="#10b981" strokeWidth="1.5" markerEnd="url(#issuer-ag)" />
      <text x={(cX + sX) / 2} y="252" textAnchor="middle" fontSize="9" fontWeight="600" fill="#065f46">Connect with Actor credential</text>
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
      description="Checks app membership and issues Actor credentials, TURN credentials and relay identities."
    >
      <StatusSection
        enabled={data.enabled}
        healthy={data.status?.is_healthy}
        disabledHint={<>This service is not enabled. Set the Issuer bit (bit 3) in the <code>enable</code> bitmask to activate it.</>}
      />

      <ServiceAccessCard service="issuer" data={data} onRefresh={fetchData} />

      {data.enabled && <ServiceMetrics status={data.status} storageKey="issuer" />}

      {data.config && (
        <HowItWorks storageKey="issuer">
          <p className="text-xs text-gray-500 mb-4">
            For <code>/issuer/register</code>, Issuer checks the client’s signed proof against active App membership
            and checks that the client can register the requested workload. Only an Owner can register the
            App’s selected Service. Package registration checks package provenance. Linked registration uses
            its own checks and does not claim a publisher identity.
            Issuer reserves an <code>ActrId</code> and requests a signature for the <code>AIdCredential</code> from Signer using the
            current issuer key for that Realm. It returns the Actor credential, a short-lived TURN credential,
            a renewal token and connection settings.
          </p>
          <IssuerDiagram />

          <div className="mt-4 space-y-2 border-t border-gray-100 pt-4 text-xs text-gray-500">
            <p className="font-semibold text-gray-600">After registration</p>
            <ul className="list-disc space-y-1.5 pl-4">
              <li>
                <strong className="text-gray-600">Renew or recover.</strong> <code>/issuer/renew</code> uses signed
                client proof and a valid renewal token to refresh credentials for the same <code>ActrId</code>.
                <code className="ml-1">/issuer/reissue</code> can recover credentials for that identity by
                reauthenticating and proving the binding to the existing Actor.
              </li>
              <li>
                <strong className="text-gray-600">Relay identity.</strong> <code>/issuer/relay-identity</code> is
                a separate request with Actor proof and a valid renewal token. Its signed relay identity is used
                to authenticate to WSS Relay.
              </li>
              <li>
                <strong className="text-gray-600">Lifetimes.</strong> <code>token_ttl_secs</code> sets the lifetime
                for the Actor and TURN credentials issued here. The renewal token has its own expiry.
                <code className="ml-1">signaling_heartbeat_interval_secs</code> tells the client how often to send
                a heartbeat to Signaling.
              </li>
              <li>
                <strong className="text-gray-600">Key rotation.</strong> The first issuer key is provisioned in
                Signer. The signed Realm Profile publishes a <code>next</code> key before Issuer switches to it.
                The old key can still verify credentials during their validity and clock-skew allowance.
              </li>
            </ul>
          </div>
        </HowItWorks>
      )}
    </ServicePageLayout>
  );
}
