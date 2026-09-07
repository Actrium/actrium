import { useEffect, useState, useCallback } from "react";
import { api, type RealmInfo } from "../lib/api";
import { RealmTable } from "../components/realms/RealmTable";
import { RealmForm } from "../components/realms/RealmForm";
import { HowItWorks } from "../components/ui/HowItWorks";
import { ServicePageLayout } from "../components/layout/ServicePageLayout";

/* ── Realms page ─────────────────────────────────────────────── */

import { ConfirmModal } from "../components/ui/ConfirmModal";
import { RealmSecretModal } from "../components/realms/RealmSecretModal";

const REALM_WRITES_DISABLED_MESSAGE =
  "Realm writes are managed by superv while NodeAdminService gRPC API is enabled.";

function SingleRealmDiagram() {
  return (
    <div className="mx-auto max-w-2xl rounded-xl border border-blue-100 bg-blue-50 p-5 text-sm text-blue-900">
      <div className="font-semibold">One Actrix instance → one Realm</div>
      <div className="mt-2 grid gap-2 sm:grid-cols-5 text-center text-xs">
        {["Authority", "Signaling", "STUN / TURN", "Publisher Registry", "Management"].map((plane) => (
          <div key={plane} className="rounded-lg border border-blue-200 bg-white px-2 py-3">{plane}</div>
        ))}
      </div>
      <p className="mt-3 text-xs text-blue-700">
        These are separate trust planes but may share one process in the v1 all-in-one profile.
        Re-provisioning a different Realm is an offline instance operation, not an in-place delete.
      </p>
    </div>
  );
}

export function Realms() {
  const [realms, setRealms] = useState<RealmInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [realmWritesEnabled, setRealmWritesEnabled] = useState(true);
  const [realmSecretNotice, setRealmSecretNotice] = useState<{
    realmId: number;
    secret: string;
    previousValidUntil?: number | null;
  } | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [editingRealm, setEditingRealm] = useState<RealmInfo | null>(null);
  const [rotateConfirm, setRotateConfirm] = useState<{ isOpen: boolean; realmId: number | null }>({
    isOpen: false,
    realmId: null,
  });

  const ensureRealmWritesEnabled = useCallback(() => {
    if (realmWritesEnabled) {
      return true;
    }
    setError(REALM_WRITES_DISABLED_MESSAGE);
    return false;
  }, [realmWritesEnabled]);

  const fetchCapabilities = useCallback(async () => {
    try {
      const capabilities = await api.getCapabilities();
      setRealmWritesEnabled(capabilities.realm_writes_enabled);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load admin capabilities");
    }
  }, []);

  const fetchRealms = useCallback(async () => {
    try {
      const data = await api.listRealms();
      setRealms(data.realms);
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load realms");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchRealms();
  }, [fetchRealms]);

  useEffect(() => {
    fetchCapabilities();
  }, [fetchCapabilities]);

  useEffect(() => {
    if (realmWritesEnabled) {
      return;
    }
    setShowForm(false);
    setEditingRealm(null);
    setRotateConfirm({ isOpen: false, realmId: null });
  }, [realmWritesEnabled]);

  async function handleToggleEnabled(realm: RealmInfo) {
    if (!ensureRealmWritesEnabled()) return;

    try {
      await api.updateRealm(realm.realm_id, { enabled: !realm.enabled });
      await fetchRealms();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update realm");
    }
  }

  async function handleRotateSecretClick(realmId: number) {
    if (!ensureRealmWritesEnabled()) return;
    setRotateConfirm({ isOpen: true, realmId });
  }

  async function handleConfirmRotate() {
    if (!ensureRealmWritesEnabled()) {
      setRotateConfirm({ isOpen: false, realmId: null });
      return;
    }

    const realmId = rotateConfirm.realmId;
    if (realmId === null) return;

    try {
      const resp = await api.rotateRealmSecret(realmId);
      setRealmSecretNotice({
        realmId: resp.realm_id,
        secret: resp.realm_secret,
        previousValidUntil: resp.previous_valid_until,
      });
      setRotateConfirm({ isOpen: false, realmId: null });
      await fetchRealms();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to rotate realm secret");
      setRotateConfirm({ isOpen: false, realmId: null });
    }
  }

  function handleEdit(realm: RealmInfo) {
    if (!ensureRealmWritesEnabled()) return;

    setEditingRealm(realm);
    setShowForm(true);
  }

  function handleFormClose() {
    setShowForm(false);
    setEditingRealm(null);
  }

  async function handleFormSubmit(data: {
    realm_id?: number;
    name: string;
    enabled: boolean;
  }) {
    if (!ensureRealmWritesEnabled()) {
      throw new Error(REALM_WRITES_DISABLED_MESSAGE);
    }

    try {
      if (editingRealm) {
        await api.updateRealm(editingRealm.realm_id, {
          name: data.name,
          enabled: data.enabled,
        });
        handleFormClose();
        await fetchRealms();
        return; // Return void for update
      } else {
        const resp = await api.createRealm(data);
        await fetchRealms(); // Refresh list immediately in background
        return resp; // Return response for secret display
      }
    } catch (err) {
      throw err;
    }
  }

  if (loading) {
    return <div className="text-sm text-gray-500">Loading...</div>;
  }

  return (
    <ServicePageLayout
      title="Realm"
      description="The single identity and trust boundary hosted by this Actrix instance"
      headerActions={
        <button
          onClick={() => {
            if (ensureRealmWritesEnabled()) {
              setShowForm(true);
            }
          }}
          disabled={!realmWritesEnabled || realms.length > 0}
          title={realms.length > 0 ? "This instance already hosts its Realm" : (realmWritesEnabled ? "Provision Realm" : "Managed by superv")}
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 disabled:text-gray-500"
        >
          Provision Realm
        </button>
      }
    >
      <HowItWorks storageKey="realms">
        <p className="text-xs text-gray-500 mb-4">
          The v1 production profile hosts exactly one Realm per Actrix instance. The Realm
          registration secret is an administrator/host capability: AIS verifies it on every
          registration, and guest workloads must never receive it directly.
        </p>
        <SingleRealmDiagram />

        <div className="mt-5 space-y-2 text-xs text-gray-500 border-t border-gray-100 pt-4">
          <p className="font-semibold text-gray-600">Key concepts</p>
          <ul className="list-disc pl-4 space-y-1.5">
            <li>
              <strong className="text-gray-600">Single root</strong> — the system administrator is
              the only Realm-management root for this instance.
            </li>
            <li>
              <strong className="text-gray-600">Default deny</strong> — Actor discovery and relay
              require an explicit ACL rule; an absent policy does not grant access.
            </li>
            <li>
              <strong className="text-gray-600">Logical planes</strong> — Authority, rendezvous,
              relay, registry, and management remain distinct even in one process.
            </li>
            <li>
              <strong className="text-gray-600">No federation in v1</strong> — third-party provider
              delegation and cross-Realm federation are intentionally out of scope.
            </li>
            <li>
              <strong className="text-gray-600">Realm secret</strong> — every realm has a secret
              issued at creation. Actors must present it at registration; AIS verifies the
              hash before issuing credentials. Secrets can be rotated with a 4h dual-validity window.
            </li>
            <li>
              <strong className="text-gray-600">Lifecycle</strong> — suspended or expired realms
              block all authentication. Actors from inactive realms cannot connect to any service.
            </li>
          </ul>
        </div>
      </HowItWorks>

      {error && (
        <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error}
        </div>
      )}

      {!realmWritesEnabled && (
        <div className="rounded-lg border border-blue-200 bg-blue-50 p-3 text-sm text-blue-800">
          Realm writes are disabled because this node is managed by superv through NodeAdminService.
        </div>
      )}

      {/* realmSecretNotice block removed in favor of modal */}

      <RealmTable
        realms={realms}
        writesEnabled={realmWritesEnabled}
        onToggleEnabled={handleToggleEnabled}
        onEdit={handleEdit}
        onRotateSecret={handleRotateSecretClick}
      />

      {showForm && (
        <RealmForm
          realm={editingRealm}
          onSubmit={handleFormSubmit}
          onClose={handleFormClose}
          onRefresh={fetchRealms}
        />
      )}

      <ConfirmModal
        isOpen={rotateConfirm.isOpen}
        title="Rotate Realm Secret"
        message={
          <span>
            Are you sure you want to rotate the secret for realm <strong>{rotateConfirm.realmId}</strong>?
            <br />
            <br />
            <span className="text-xs text-gray-500 block">
              The old secret will remain valid for 4 hours to allow graceful rotation.
              After confirmation, the new secret will be displayed once.
            </span>
          </span>
        }
        confirmLabel="Rotate Secret"
        cancelLabel="Cancel"
        isDestructive={false}
        onConfirm={handleConfirmRotate}
        onCancel={() => setRotateConfirm({ isOpen: false, realmId: null })}
      />

      {realmSecretNotice && (
        <RealmSecretModal
          isOpen={!!realmSecretNotice}
          realmId={realmSecretNotice.realmId}
          secret={realmSecretNotice.secret}
          previousValidUntil={realmSecretNotice.previousValidUntil}
          onClose={() => setRealmSecretNotice(null)}
        />
      )}
    </ServicePageLayout>
  );
}
