import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, type AdminCapabilities, type RealmInfo } from "../lib/api";
import { RealmTable } from "../components/realms/RealmTable";
import { SignedAppRequest } from "../components/realms/SignedAppRequest";
import { RealmForm } from "../components/realms/RealmForm";
import { ServicePageLayout } from "../components/layout/ServicePageLayout";
import { IdentityReview } from "../components/identity/IdentityReview";

export function Realms() {
  const navigate = useNavigate();
  const [apps, setApps] = useState<RealmInfo[]>([]);
  const [caps, setCaps] = useState<AdminCapabilities | null>(null);
  const [error, setError] = useState("");
  const [showForm, setShowForm] = useState(false);
  const refresh = useCallback(async () => {
    const [list, capabilities] = await Promise.all([api.listRealms(), api.getCapabilities()]);
    setApps(list.realms);
    setCaps(capabilities);
    setError("");
  }, []);
  useEffect(() => {
    refresh().catch((e) => setError(e.message));
  }, [refresh]);
  return (
    <ServicePageLayout
      title="Apps"
      description="Applications hosted by this node. Each App ID is its full Realm name."
      headerActions={
        <button
          disabled={!caps?.realm_writes_enabled}
          onClick={() => setShowForm(true)}
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white disabled:opacity-50"
        >
          Create app
        </button>
      }
    >
      {error && (
        <p role="alert" className="text-red-700">
          {error}
        </p>
      )}
      {caps?.superv_managed && (
        <p className="text-sm text-gray-600">superv manages application changes on this node.</p>
      )}
      {caps && <RealmTable realms={apps} onOpen={(id) => navigate("/admin/apps/" + encodeURIComponent(id))} />}
      <SignedAppRequest
        disabled={!caps?.realm_writes_enabled}
        onAccepted={() => void refresh().catch((e) => setError(e.message))}
      />
      <IdentityReview kind="realm-domains" disabled={!caps?.realm_writes_enabled} />
      {showForm && caps?.realm_writes_enabled && (
        <RealmForm
          provider={caps.provider_id}
          onSubmit={async (data) => {
            const response = await api.createRealm(data);
            void refresh().catch((e) => setError(e.message));
            return response;
          }}
          onClose={() => setShowForm(false)}
          onRefresh={() => {
            void refresh().catch((e) => setError(e.message));
          }}
        />
      )}
    </ServicePageLayout>
  );
}
