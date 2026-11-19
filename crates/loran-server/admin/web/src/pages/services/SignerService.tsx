import { ServiceAccessCard } from "./ServiceAccessCard";
import { useEffect, useState, useCallback } from "react";
import { api, type ServiceDetail, type KeyEntry } from "../../lib/api";
import { ServicePageLayout, StatusSection } from "../../components/layout/ServicePageLayout";
import { HowItWorks } from "../../components/ui/HowItWorks";
import { ServiceMetrics } from "./shared";
import { CollapsibleCard } from "../../components/ui/CollapsibleCard";

export function SignerService() {
  const [data, setData] = useState<ServiceDetail | null>(null);
  const [keys, setKeys] = useState<KeyEntry[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [error, setError] = useState("");
  const [keyError, setKeyError] = useState("");
  const fetchData = useCallback(async () => {
    const [detail, inventory] = await Promise.allSettled([api.getServiceDetail("signer"), api.getSignerKeys()]);
    if (detail.status === "fulfilled") {
      setData(detail.value);
      setError("");
    } else {
      setData(null);
      setError("Signer status unavailable.");
    }
    if (inventory.status === "fulfilled") {
      setKeys(inventory.value.keys);
      setTotalCount(inventory.value.total_count);
      setKeyError("");
    } else {
      setKeyError("Key inventory unavailable.");
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, [fetchData]);

  if (!data && !error) {
    return <div className="text-sm text-gray-500">Loading...</div>;
  }

  return (
    <ServicePageLayout
      title="Signer Service"
      description="Holds Ed25519 private keys and signs requests from authorized services, including the Admin device an Owner has registered."
    >
      {error && !data && (
        <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
          {error}
        </div>
      )}

      {data && (
        <StatusSection
          enabled={data.enabled}
          healthy={data.status?.is_healthy}
          disabledHint={
            <>
              This service is not enabled. Set the Signer bit (bit 4) in the <code>enable</code> bitmask to activate it.
              The flow below shows how an enabled Signer is used.
            </>
          }
        />
      )}

      {data && <ServiceAccessCard service="signer" data={data} onRefresh={fetchData} />}

      {data?.enabled && <ServiceMetrics status={data.status} storageKey="signer" />}

      <HowItWorks storageKey="signer_v3" defaultExpanded>
        <p className="mb-4 text-xs text-gray-500">
          Signer keeps Ed25519 private keys in its configured storage and performs signing over authenticated
          gRPC. Its API returns public keys and signatures. Issuer handles App membership and credential policy.
        </p>
        <ol className="grid gap-3 lg:grid-cols-3" aria-label="Actor credential signing and verification">
          <li className="rounded-lg border border-blue-100 bg-blue-50 p-3">
            <h3 className="mb-1 text-sm font-semibold text-blue-900">1. Issuer prepares the claims</h3>
            <p className="text-xs text-blue-900">
              Issuer checks the client and selects the current signing key for its Actr Realm.
            </p>
          </li>
          <li className="rounded-lg border border-violet-100 bg-violet-50 p-3">
            <h3 className="mb-1 text-sm font-semibold text-violet-900">2. Signer signs</h3>
            <p className="text-xs text-violet-900">
              Signer returns an Ed25519 signature. Issuer returns the claims, <code>key_id</code> and signature
              in the Actor credential.
            </p>
          </li>
          <li className="rounded-lg border border-emerald-100 bg-emerald-50 p-3">
            <h3 className="mb-1 text-sm font-semibold text-emerald-900">3. Signaling verifies</h3>
            <p className="text-xs text-emerald-900">
              Signaling verifies the signature locally using a public key for the Realm and <code>key_id</code>.
              It also checks the credential expiry and whether the Actor and client key are still active.
            </p>
          </li>
        </ol>
        <ul className="mt-4 list-disc space-y-2 border-t border-gray-100 pl-4 pt-4 text-xs text-gray-500">
          <li>
            <strong className="text-gray-600">Public keys and rotation.</strong> Issuer makes verification keys
            available through the local key cache. The signed Realm Profile can list <code>next</code>,
            <code className="ml-1">current</code> and <code>retired</code> issuer keys. Only the current key signs
            new Actor credentials. A retired key can still verify previously issued credentials during the
            rotation overlap.
          </li>
          <li>
            <strong className="text-gray-600">Admin device.</strong> An App Owner can authorize an AFP Admin
            device through a device invitation. AFP uses Signer to sign operations with the device’s own key.
            AFP checks device enrollment and invitation delivery locally against Realm membership.
            Other member actions go through <code>/issuer/membership</code>.
          </li>
          <li>
            <strong className="text-gray-600">TURN credentials.</strong> In the Actr Realm mode, TURN validates its
            own short-lived credential, checks its binding to the active Actor and device, and does not use the
            Actor credential’s Ed25519 signature.
          </li>
        </ul>
      </HowItWorks>

      {data?.enabled && (
        <CollapsibleCard storageKey="signer_keys" title="Keys">
        <div className="mb-3">
          <span className="text-xs text-gray-500">
            {keyError || (keys.length + " of " + totalCount + " total")}
          </span>
        </div>
        {keyError ? null : keys.length === 0 ? (
          <p className="text-sm text-gray-500">No keys found</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-gray-200 text-left">
                  <th className="pb-2 pr-4 font-medium text-gray-500">Key ID</th>
                  <th className="pb-2 pr-4 font-medium text-gray-500">PK Size</th>
                  <th className="pb-2 pr-4 font-medium text-gray-500">Created At</th>
                  <th className="pb-2 font-medium text-gray-500">Status</th>
                </tr>
              </thead>
              <tbody>
                {keys.map((k) => (
                  <tr key={k.key_id} className="border-b border-gray-100 last:border-0">
                    <td className="py-2 pr-4 font-mono">{k.key_id}</td>
                    <td className="py-2 pr-4">{k.pk_size} bytes</td>
                    <td className="py-2 pr-4 font-mono text-xs">
                      {new Date(k.created_at * 1000).toLocaleString()}
                    </td>
                    <td className="py-2">
                      {k.retired ? (
                        <span className="inline-flex items-center rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-600">
                          Retired
                        </span>
                      ) : (
                        <span className="inline-flex items-center rounded-full bg-green-100 px-2 py-0.5 text-xs font-medium text-green-700">
                          Active
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
      )}
    </ServicePageLayout>
  );
}
