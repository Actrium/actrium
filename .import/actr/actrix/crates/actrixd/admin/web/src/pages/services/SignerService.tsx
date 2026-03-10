import { useEffect, useState, useCallback } from "react";
import { api, type ServiceDetail, type KeyEntry } from "../../lib/api";
import { ServicePageLayout, ConfigSection, StatusSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import { ServiceMetrics } from "./shared";
import { CollapsibleCard } from "../../components/ui/CollapsibleCard";

function SignerLifecycleDiagram({ config }: { config: Record<string, unknown> }) {
  const ttl = Number(config.key_ttl_seconds ?? 3600);
  const tolerance = Number(config.tolerance_seconds ?? 300);

  // Timeline proportions
  const totalSpan = ttl + tolerance;
  const activeRatio = ttl / totalSpan;

  // Layout constants
  const padL = 30;
  const barW = 520;
  const barH = 28;

  // Top-section columns
  const aisX = 80;    // AIS center
  const sgX = 280;    // Signer center
  const cacheX = 480; // KeyCache center
  const verX = 640;   // Signaling/TURN center

  // Lifecycle section y-offsets (shifted down for taller top section)
  const dividerY = 210;
  const lcTitleY = dividerY + 20;
  const barY = dividerY + 58;

  const totalH = barY + barH + 48;
  const activeW = Math.round(barW * activeRatio);
  const toleranceW = barW - activeW;

  return (
    <svg
      viewBox={`0 0 720 ${totalH}`}
      className="max-w-3xl mx-auto"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <marker id="sgn-ab" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#3b82f6" />
        </marker>
        <marker id="sgn-ag" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#10b981" />
        </marker>
        <marker id="sgn-ao" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#d97706" />
        </marker>
        <marker id="sgn-ap" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#8b5cf6" />
        </marker>
        <marker id="sgn-ar" markerWidth="7" markerHeight="5" refX="7" refY="2.5" orient="auto">
          <path d="M0,0 L7,2.5 L0,5" fill="#9ca3af" />
        </marker>
      </defs>

      {/* ═══════════════════════════════════════════════════ */}
      {/* Section 1: Issuance — AIS ↔ Signer ↔ SQLite       */}
      {/* ═══════════════════════════════════════════════════ */}

      {/* AIS box */}
      <rect x={aisX - 50} y="10" width="100" height="40" rx="8" fill="#e0e7ff" stroke="#6366f1" strokeWidth="1.5" />
      <text x={aisX} y="28" textAnchor="middle" fontSize="11" fontWeight="600" fill="#3730a3">AIS</text>
      <text x={aisX} y="41" textAnchor="middle" fontSize="8" fill="#6366f1">Identity Issuer</text>

      {/* Signer box */}
      <rect x={sgX - 56} y="10" width="112" height="40" rx="8" fill="#fef3c7" stroke="#d97706" strokeWidth="1.5" />
      <text x={sgX} y="28" textAnchor="middle" fontSize="11" fontWeight="600" fill="#92400e">Signer</text>
      <text x={sgX} y="41" textAnchor="middle" fontSize="8" fill="#d97706">Signing Oracle</text>

      {/* SQLite box (below Signer) */}
      <rect x={sgX - 30} y="64" width="60" height="22" rx="4" fill="#f1f5f9" stroke="#94a3b8" strokeWidth="1" />
      <text x={sgX} y="79" textAnchor="middle" fontSize="8" fontWeight="600" fill="#475569">SQLite</text>
      <line x1={sgX} y1="50" x2={sgX} y2="64" stroke="#94a3b8" strokeWidth="1" markerEnd="url(#sgn-ar)" />
      <text x={sgX + 36} y="73" fontSize="6.5" fill="#94a3b8">signing keys</text>

      {/* ① AIS → Signer: GenerateSigningKey */}
      <line x1={aisX + 50} y1="20" x2={sgX - 58} y2="20" stroke="#3b82f6" strokeWidth="1.5" markerEnd="url(#sgn-ab)" />
      <text x={(aisX + sgX) / 2} y="16" textAnchor="middle" fontSize="7.5" fontWeight="600" fill="#3b82f6">① GenerateSigningKey</text>

      {/* Signer → AIS: key_id + verifying_key */}
      <line x1={sgX - 58} y1="32" x2={aisX + 50} y2="32" stroke="#10b981" strokeWidth="1.5" markerEnd="url(#sgn-ag)" />
      <text x={(aisX + sgX) / 2} y="44" textAnchor="middle" fontSize="7" fill="#10b981">key_id + verifying_key</text>

      {/* ② AIS → Signer: Sign(key_id, msg) */}
      <line x1={aisX + 50} y1="58" x2={sgX - 58} y2="58" stroke="#6366f1" strokeWidth="1.2" strokeDasharray="3 2" markerEnd="url(#sgn-ap)" />
      <text x={(aisX + sgX) / 2} y="55" textAnchor="middle" fontSize="7.5" fontWeight="600" fill="#6366f1">② Sign(key_id, msg)</text>

      {/* Signer → AIS: 64-byte signature */}
      <line x1={sgX - 58} y1="70" x2={aisX + 50} y2="70" stroke="#10b981" strokeWidth="1.2" strokeDasharray="3 2" markerEnd="url(#sgn-ag)" />
      <text x={(aisX + sgX) / 2} y="81" textAnchor="middle" fontSize="7" fill="#10b981">64-byte Ed25519 signature</text>

      {/* Cluster-private label */}
      <text x={(aisX + sgX) / 2} y="96" textAnchor="middle" fontSize="6.5" fill="#94a3b8">
        gRPC · nonce-auth · cluster-private
      </text>

      {/* ═══════════════════════════════════════════════════ */}
      {/* Section 2: Key distribution + verification         */}
      {/* ═══════════════════════════════════════════════════ */}

      {/* KeyCache box */}
      <rect x={cacheX - 56} y="10" width="112" height="40" rx="8" fill="#ecfdf5" stroke="#22c55e" strokeWidth="1.2" />
      <text x={cacheX} y="28" textAnchor="middle" fontSize="10" fontWeight="600" fill="#166534">KeyCache</text>
      <text x={cacheX} y="41" textAnchor="middle" fontSize="7" fill="#16a34a">shared SQLite</text>

      {/* Signaling / TURN box */}
      <rect x={verX - 48} y="10" width="96" height="40" rx="8" fill="#f5f3ff" stroke="#8b5cf6" strokeWidth="1.2" />
      <text x={verX} y="24" textAnchor="middle" fontSize="9" fontWeight="600" fill="#5b21b6">Signaling</text>
      <text x={verX} y="35" textAnchor="middle" fontSize="9" fontWeight="600" fill="#5b21b6">TURN</text>
      <text x={verX} y="46" textAnchor="middle" fontSize="7" fill="#a78bfa">verifiers</text>

      {/* ③ AIS → KeyCache: persist verifying key */}
      <line x1={aisX + 50} y1="100" x2={cacheX - 58} y2="100" stroke="#22c55e" strokeWidth="1.2" markerEnd="url(#sgn-ag)" />
      <text x={(aisX + cacheX) / 2} y="97" textAnchor="middle" fontSize="7.5" fontWeight="600" fill="#22c55e">③ persist_key(key_id, verifying_key)</text>

      {/* AIS lifeline down to step ③ */}
      <line x1={aisX} y1="50" x2={aisX} y2="100" stroke="#c7d2fe" strokeWidth="1" strokeDasharray="4 3" />

      {/* Actor Peer box (center-bottom) */}
      <rect x={aisX - 50} y="132" width="100" height="34" rx="6" fill="#dbeafe" stroke="#3b82f6" strokeWidth="1.2" />
      <text x={aisX} y="148" textAnchor="middle" fontSize="9" fontWeight="600" fill="#1e40af">Actor Peer</text>
      <text x={aisX} y="160" textAnchor="middle" fontSize="7" fill="#60a5fa">client</text>

      {/* ④ AIS → Actor: AIdCredential */}
      <line x1={aisX} y1="106" x2={aisX} y2="132" stroke="#3b82f6" strokeWidth="1.2" markerEnd="url(#sgn-ab)" />
      <text x={aisX + 60} y="120" fontSize="7.5" fontWeight="600" fill="#3b82f6">④ AIdCredential</text>
      <text x={aisX + 60} y="130" fontSize="6.5" fill="#64748b">key_id + claims + signature</text>

      {/* ⑤ Actor → Signaling/TURN: present credential */}
      <line x1={aisX + 50} y1="149" x2={verX - 50} y2="149" stroke="#8b5cf6" strokeWidth="1.2" markerEnd="url(#sgn-ap)" />
      <text x={(aisX + verX) / 2} y="143" textAnchor="middle" fontSize="7.5" fontWeight="600" fill="#8b5cf6">⑤ present AIdCredential</text>

      {/* ⑥ Signaling/TURN → KeyCache: lookup verifying key */}
      <line x1={verX - 48} y1="60" x2={cacheX + 58} y2="60" stroke="#22c55e" strokeWidth="1.2" markerEnd="url(#sgn-ag)" />
      <text x={(verX + cacheX) / 2} y="57" textAnchor="middle" fontSize="7.5" fontWeight="600" fill="#22c55e">⑥ lookup(key_id)</text>

      {/* KeyCache → Signaling/TURN: verifying key */}
      <line x1={cacheX + 58} y1="72" x2={verX - 48} y2="72" stroke="#22c55e" strokeWidth="1.2" strokeDasharray="3 2" markerEnd="url(#sgn-ag)" />
      <text x={(verX + cacheX) / 2} y="84" textAnchor="middle" fontSize="7" fill="#16a34a">Ed25519 verifying key</text>

      {/* Verification lifelines */}
      <line x1={verX} y1="50" x2={verX} y2="149" stroke="#ddd6fe" strokeWidth="1" strokeDasharray="4 3" />

      {/* GetVerifyingKey (remote/cluster) note */}
      <rect x={sgX + 62} y="130" width="138" height="28" rx="5" fill="#faf5ff" stroke="#ddd6fe" strokeWidth="0.8" />
      <text x={sgX + 131} y="143" textAnchor="middle" fontSize="7" fill="#8b5cf6">GetVerifyingKey (gRPC)</text>
      <text x={sgX + 131} y="153" textAnchor="middle" fontSize="6.5" fill="#a78bfa">for remote/clustered verifiers</text>

      {/* Auth note */}
      <text x="360" y={dividerY - 8} textAnchor="middle" fontSize="7" fill="#94a3b8">
        Signer RPCs authenticated via nonce-auth (HMAC-SHA256 + one-time nonce)
      </text>

      {/* ═══════════════════════════════════════════════════ */}
      {/* Section 3: Key Lifecycle timeline                  */}
      {/* ═══════════════════════════════════════════════════ */}

      <line x1="16" y1={dividerY} x2="704" y2={dividerY} stroke="#e2e8f0" strokeWidth="1" />

      <text x={padL} y={lcTitleY} fontSize="10" fontWeight="600" fill="#475569">Key Lifecycle</text>
      <text x={padL} y={lcTitleY + 14} fontSize="8" fill="#94a3b8">
        total validity = {ttl}s active + {tolerance}s tolerance
      </text>

      {/* Timeline bar — Active */}
      <rect x={padL} y={barY} width={activeW} height={barH} rx="4"
        fill="#dcfce7" stroke="#22c55e" strokeWidth="1.2" />
      <text x={padL + activeW / 2} y={barY + 12} textAnchor="middle"
        fontSize="9" fontWeight="600" fill="#166534">
        Active ({ttl}s)
      </text>
      <text x={padL + activeW / 2} y={barY + 23} textAnchor="middle"
        fontSize="7" fill="#16a34a">
        sign + verify
      </text>

      {/* Timeline bar — Tolerance */}
      <rect x={padL + activeW} y={barY} width={toleranceW} height={barH} rx="4"
        fill="#fef3c7" stroke="#f59e0b" strokeWidth="1.2" />
      <text x={padL + activeW + toleranceW / 2} y={barY + 12} textAnchor="middle"
        fontSize="9" fontWeight="600" fill="#92400e">
        Tolerance ({tolerance}s)
      </text>
      <text x={padL + activeW + toleranceW / 2} y={barY + 23} textAnchor="middle"
        fontSize="7" fill="#b45309">
        verify only
      </text>

      {/* Time markers */}
      <line x1={padL} y1={barY + barH + 4} x2={padL} y2={barY + barH + 14}
        stroke="#64748b" strokeWidth="1" />
      <text x={padL} y={barY + barH + 24} textAnchor="middle" fontSize="8" fill="#64748b">
        Created
      </text>

      <line x1={padL + activeW} y1={barY + barH + 4} x2={padL + activeW} y2={barY + barH + 14}
        stroke="#64748b" strokeWidth="1" />
      <text x={padL + activeW} y={barY + barH + 24} textAnchor="middle" fontSize="8" fill="#64748b">
        Expires
      </text>

      <line x1={padL + barW} y1={barY + barH + 4} x2={padL + barW} y2={barY + barH + 14}
        stroke="#64748b" strokeWidth="1" />
      <text x={padL + barW} y={barY + barH + 24} textAnchor="middle" fontSize="8" fill="#64748b">
        Cleanup
      </text>

      {/* Cleanup note */}
      <text x={padL + barW + 12} y={barY + barH + 24} fontSize="7" fill="#94a3b8">
        lazy, every 100 reqs, min 10 keys
      </text>
    </svg>
  );
}

export function SignerService() {
  const [data, setData] = useState<ServiceDetail | null>(null);
  const [keys, setKeys] = useState<KeyEntry[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [error, setError] = useState("");
  const [cleaning, setCleaning] = useState(false);
  const [cleanupMsg, setCleanupMsg] = useState("");

  const fetchData = useCallback(async () => {
    try {
      const [d, k] = await Promise.all([
        api.getServiceDetail("signer"),
        api.getSignerKeys().catch(() => ({ keys: [], total_count: 0 })),
      ]);
      setData(d);
      setKeys(k.keys);
      setTotalCount(k.total_count);
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
      title="Signer Service"
      description="Signing Oracle — generates Ed25519 key pairs and signs on behalf of AIS; private keys never leave the process"
    >
      <StatusSection
        enabled={data.enabled}
        healthy={data.status?.is_healthy}
        disabledHint={<>This service is not enabled. Set the Signer bit (bit 4) in the <code>enable</code> bitmask to activate it.</>}
      />

      {data.enabled && <ServiceMetrics status={data.status} storageKey="signer" />}

      {data.config && (
        <HowItWorks storageKey="signer">
          <p className="text-xs text-gray-500 mb-4">
            Signer is a cluster-private signing oracle. AIS calls <strong>①&nbsp;GenerateSigningKey</strong> to
            create an Ed25519 key pair — the signing key (private) stays in Signer's SQLite and is never
            exposed. When issuing credentials, AIS calls <strong>②&nbsp;Sign</strong> with
            a <code className="text-[11px] bg-gray-100 px-1 rounded">key_id</code> and message; Signer returns a
            64-byte signature. AIS then <strong>③&nbsp;persists the verifying key</strong> (public) into a
            shared KeyCache (SQLite) so local verifiers can look it up. The issued
            <strong> ④&nbsp;AIdCredential</strong> (key_id + claims + signature) is sent to the Actor Peer, who
            later <strong>⑤&nbsp;presents</strong> it to Signaling or TURN.
            Those verifiers <strong>⑥&nbsp;lookup</strong> the verifying key from the shared KeyCache by key_id
            and verify the Ed25519 signature locally — no round-trip to Signer needed.
            A separate <strong>GetVerifyingKey</strong> gRPC exists for remote/clustered verifiers
            that don't share the local KeyCache.
          </p>
          <SignerLifecycleDiagram config={data.config} />

          <div className="mt-5 space-y-2 text-xs text-gray-500 border-t border-gray-100 pt-4">
            <p className="font-semibold text-gray-600">Key concepts</p>
            <ul className="list-disc pl-4 space-y-1.5">
              <li>
                <strong className="text-gray-600">Cluster-private</strong> — Signer is never exposed to
                external clients. Only AIS communicates with it via authenticated gRPC (nonce-auth).
              </li>
              <li>
                <strong className="text-gray-600">Private key isolation</strong> — Ed25519 signing keys
                never leave Signer. AIS receives only the verifying key (public) and signatures;
                all signing happens inside Signer's process boundary.
              </li>
              <li>
                <strong className="text-gray-600">KeyCache distribution</strong> — AIS persists verifying
                keys into a shared SQLite KeyCache. Signaling and TURN read from this cache to
                verify AIdCredentials locally — zero network calls to Signer at verification time.
              </li>
              <li>
                <strong className="text-gray-600">GetVerifyingKey (gRPC)</strong> — available for
                remote/clustered verifiers that can't access the local KeyCache. Not used in
                single-node deployments.
              </li>
              <li>
                <strong className="text-gray-600">key_ttl_seconds</strong> — how long a key pair is
                active for signing + verification. AIS refreshes before expiry.
              </li>
              <li>
                <strong className="text-gray-600">tolerance_seconds</strong> — grace period after
                expiry. Verification still works (old credentials remain valid);
                new signing uses a fresher key.
              </li>
            </ul>
          </div>
        </HowItWorks>
      )}

      {data.config_fields && (
        <ConfigSection storageKey="signer" fields={data.config_fields} onRefresh={fetchData} />
      )}

      <CollapsibleCard storageKey="signer_keys" title="Keys">
        <div className="flex items-center justify-between mb-3">
          <span className="text-xs text-gray-500">
            {keys.length} of {totalCount} total
          </span>
          <div className="flex items-center gap-2">
            {cleanupMsg && (
              <span className="text-xs text-green-600">{cleanupMsg}</span>
            )}
            <button
              onClick={async () => {
                setCleaning(true);
                setCleanupMsg("");
                try {
                  const r = await api.cleanupSignerKeys();
                  setCleanupMsg(`Deleted ${r.deleted}, ${r.remaining} remaining`);
                  fetchData();
                } catch {
                  setCleanupMsg("Cleanup failed");
                } finally {
                  setCleaning(false);
                }
              }}
              disabled={cleaning}
              className="rounded-md border border-gray-300 px-2.5 py-1 text-xs font-medium text-gray-600 hover:bg-gray-50 disabled:opacity-50 transition-colors"
            >
              {cleaning ? "Cleaning..." : "Cleanup expired"}
            </button>
          </div>
        </div>
        {keys.length === 0 ? (
          <p className="text-sm text-gray-500">No keys found</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 text-left">
                  <th className="pb-2 pr-4 font-medium text-gray-500">Key ID</th>
                  <th className="pb-2 pr-4 font-medium text-gray-500">PK Size</th>
                  <th className="pb-2 pr-4 font-medium text-gray-500">Created At</th>
                  <th className="pb-2 pr-4 font-medium text-gray-500">Expires At</th>
                  <th className="pb-2 font-medium text-gray-500">Status</th>
                </tr>
              </thead>
              <tbody>
                {keys.map((k) => (
                  <tr key={k.key_id} className="border-b border-gray-100 last:border-0">
                    <td className="py-2 pr-4 font-mono">{k.key_id}</td>
                    <td className="py-2 pr-4">{k.pk_size} bytes</td>
                    <td className="py-2 pr-4 font-mono text-xs">
                      {k.created_at ? new Date(k.created_at * 1000).toLocaleString() : "—"}
                    </td>
                    <td className="py-2 pr-4 font-mono text-xs">
                      {k.expires_at === 0
                        ? "Never"
                        : new Date(k.expires_at * 1000).toLocaleString()}
                    </td>
                    <td className="py-2">
                      {k.expires_at === 0 ? (
                        <span className="inline-flex items-center rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-700">
                          Permanent
                        </span>
                      ) : k.is_expired ? (
                        <span className="inline-flex items-center rounded-full bg-red-100 px-2 py-0.5 text-xs font-medium text-red-700">
                          Expired
                        </span>
                      ) : (
                        <span className="inline-flex items-center rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-700">
                          Valid
                        </span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CollapsibleCard>
    </ServicePageLayout>
  );
}
