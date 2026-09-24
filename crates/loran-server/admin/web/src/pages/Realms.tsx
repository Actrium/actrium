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

export function Realms() {
  const [realms, setRealms] = useState<RealmInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [realmWritesEnabled, setRealmWritesEnabled] = useState(true);
  const [realmSecretNotice, setRealmSecretNotice] = useState<{
    realm: string;
    secret: string;
    previousValidUntil?: number | null;
  } | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [rotateConfirm, setRotateConfirm] = useState<{ isOpen: boolean; realm: string | null }>({
    isOpen: false,
    realm: null,
  });
  const [retireConfirm, setRetireConfirm] = useState<string | null>(null);

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
    setRotateConfirm({ isOpen: false, realm: null });
    setRetireConfirm(null);
  }, [realmWritesEnabled]);

  async function handleToggleEnabled(realm: RealmInfo) {
    if (!ensureRealmWritesEnabled()) return;

    try {
      await api.updateRealm(realm.realm, { enabled: !realm.enabled });
      await fetchRealms();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update realm");
    }
  }

  async function handleConfirmRetire() {
    const realm = retireConfirm;
    setRetireConfirm(null);
    if (realm === null || !ensureRealmWritesEnabled()) return;

    try {
      await api.retireRealm(realm);
      await fetchRealms();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to retire realm");
    }
  }

  async function handleRotateSecretClick(realm: string) {
    if (!ensureRealmWritesEnabled()) return;
    setRotateConfirm({ isOpen: true, realm });
  }

  async function handleConfirmRotate() {
    if (!ensureRealmWritesEnabled()) {
      setRotateConfirm({ isOpen: false, realm: null });
      return;
    }

    const realm = rotateConfirm.realm;
    if (realm === null) return;

    try {
      const resp = await api.rotateRealmSecret(realm);
      setRealmSecretNotice({
        realm: resp.realm,
        secret: resp.realm_secret,
        previousValidUntil: resp.previous_valid_until,
      });
      setRotateConfirm({ isOpen: false, realm: null });
      await fetchRealms();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to rotate realm secret");
      setRotateConfirm({ isOpen: false, realm: null });
    }
  }

  async function handleCreate(data: { realm: string; enabled: boolean }) {
    if (!ensureRealmWritesEnabled()) {
      throw new Error(REALM_WRITES_DISABLED_MESSAGE);
    }
    const resp = await api.createRealm(data);
    await fetchRealms();
    return resp;
  }

  if (loading) {
    return <div className="text-sm text-gray-500">Loading...</div>;
  }

  return (
    <ServicePageLayout
      title="Realms"
      description="Identity and trust boundaries hosted by this Actrix instance"
      headerActions={
        <button
          onClick={() => {
            if (ensureRealmWritesEnabled()) {
              setShowForm(true);
            }
          }}
          disabled={!realmWritesEnabled}
          title={realmWritesEnabled ? "Create Realm" : "Managed by superv"}
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 disabled:text-gray-500"
        >
          Create Realm
        </button>
      }
    >
      <HowItWorks storageKey="realms">
        <p className="text-xs text-gray-500 mb-4">
          One Actrix instance hosts many Realms. A Realm is one deployed application instance,
          named <code>&lt;realm_id&gt;@&lt;domain&gt;</code>. Realms share this instance&apos;s
          services but never see each other&apos;s Actors. The Realm registration secret is an
          administrator/host capability: Issuer verifies it on every registration, and guest
          workloads must never receive it directly.
        </p>

        <div className="mt-5 space-y-2 text-xs text-gray-500 border-t border-gray-100 pt-4">
          <p className="font-semibold text-gray-600">Key concepts</p>
          <ul className="list-disc pl-4 space-y-1.5">
            <li>
              <strong className="text-gray-600">Isolation</strong> — Actors in different Realms
              never discover or relay to each other, whatever the ACL rules say.
            </li>
            <li>
              <strong className="text-gray-600">Default deny</strong> — within a Realm, Actor
              discovery and relay require an explicit ACL rule.
            </li>
            <li>
              <strong className="text-gray-600">Realm secret</strong> — every realm has a secret
              issued at creation. Actors must present it at registration; Issuer verifies the
              hash before issuing credentials. Secrets can be rotated with a 4h dual-validity window.
            </li>
            <li>
              <strong className="text-gray-600">Lifecycle</strong> — suspending a Realm blocks all
              authentication until it is resumed. Retiring a Realm is final: its connection ACL,
              pending registrations and renewal tokens are removed, and its name is never reused.
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
        onRotateSecret={handleRotateSecretClick}
        onRetire={(realm) => {
          if (ensureRealmWritesEnabled()) setRetireConfirm(realm);
        }}
      />

      {showForm && (
        <RealmForm
          onSubmit={handleCreate}
          onClose={() => setShowForm(false)}
          onRefresh={fetchRealms}
        />
      )}

      <ConfirmModal
        isOpen={retireConfirm !== null}
        title="Retire Realm"
        message={
          <span>
            Retire realm <strong>{retireConfirm}</strong>?
            <br />
            <br />
            <span className="text-xs text-gray-500 block">
              Retirement is final. Actors of this Realm lose service, its connection ACL, pending
              registrations and renewal tokens are removed, and the name can never be used again.
            </span>
          </span>
        }
        confirmLabel="Retire Realm"
        cancelLabel="Cancel"
        isDestructive
        onConfirm={handleConfirmRetire}
        onCancel={() => setRetireConfirm(null)}
      />

      <ConfirmModal
        isOpen={rotateConfirm.isOpen}
        title="Rotate Realm Secret"
        message={
          <span>
            Are you sure you want to rotate the secret for realm <strong>{rotateConfirm.realm}</strong>?
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
        onCancel={() => setRotateConfirm({ isOpen: false, realm: null })}
      />

      {realmSecretNotice && (
        <RealmSecretModal
          isOpen={!!realmSecretNotice}
          realm={realmSecretNotice.realm}
          secret={realmSecretNotice.secret}
          previousValidUntil={realmSecretNotice.previousValidUntil}
          onClose={() => setRealmSecretNotice(null)}
        />
      )}
    </ServicePageLayout>
  );
}
