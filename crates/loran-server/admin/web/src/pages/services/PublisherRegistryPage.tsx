import { ServiceAccessCard } from "./ServiceAccessCard";
import { Fragment, useState, useEffect, useCallback, useRef, type ReactNode } from "react";
import { CheckCircle, Clock, XCircle, AlertTriangle, ShieldOff, Timer } from "lucide-react";
import { api, type ServiceDetail, publisherApi, type PublisherRecord, type PublisherKeyHistory } from '../../lib/api';
import { IdentityReview } from '../../components/identity/IdentityReview';
import { ServicePageLayout } from "../../components/layout/ServicePageLayout";
import { ConfirmModal } from "../../components/ui/ConfirmModal";
import {
  ManagementDisclosure, ManagementStatus, ManagementTable,
  managementBadge, managementButton, managementCard, managementCell, managementRow,
} from "../../components/ui/ManagementControls";

// ── Status badge ──────────────────────────────────────────────────

function StatusBadge({ status }: { status: PublisherRecord['status'] }) {
  const variants = {
    pending: { tone: "info", label: "Pending" },
    active: { tone: "success", label: "Active" },
    suspended: { tone: "warning", label: "Suspended" },
    revoked: { tone: "danger", label: "Revoked" },
  } as const;
  const { tone, label } = variants[status];
  return <ManagementStatus tone={tone}>{label}</ManagementStatus>;
}

// ── Key Expiry Badge ──────────────────────────────────────────────

function KeyExpiryBadge({ expiresAt }: { expiresAt?: number }) {
  if (!expiresAt) {
    return <span className="text-gray-300 text-xs">—</span>;
  }

  const now = Date.now() / 1000;
  const remaining = expiresAt - now;
  const days = Math.ceil(remaining / 86400);
  const expiryDate = new Date(expiresAt * 1000);
  const dateStr = expiryDate.toLocaleDateString();

  let colorClass: string;
  let label: string;
  let Icon = Timer;

  if (remaining <= 0) {
    // Expired
    colorClass = 'bg-red-600 text-white';
    label = 'Expired';
    Icon = XCircle;
  } else if (days <= 7) {
    // ≤ 7 days — critical
    colorClass = 'bg-red-100 text-red-800 ring-1 ring-red-300';
    label = `${days}d left`;
    Icon = AlertTriangle;
  } else if (days <= 15) {
    // ≤ 15 days — warning
    colorClass = 'bg-orange-100 text-orange-800 ring-1 ring-orange-300';
    label = `${days}d left`;
    Icon = AlertTriangle;
  } else if (days <= 30) {
    // ≤ 30 days — attention
    colorClass = 'bg-yellow-100 text-yellow-800 ring-1 ring-yellow-200';
    label = `${days}d left`;
    Icon = Clock;
  } else {
    // Healthy
    colorClass = 'bg-green-50 text-green-700';
    label = dateStr;
    Icon = CheckCircle;
  }

  return (
    <span
      className={managementBadge + " " + colorClass}
      title={`Expires: ${expiryDate.toLocaleString()}${remaining > 0 ? ` (${days} days remaining)` : ' (EXPIRED)'}`}
    >
      <Icon size={11} />
      {label}
    </span>
  );
}

// ── Key History Panel ────────────────────────────────────────────────

interface RegistryConfirmation {
  title: string;
  confirmLabel: string;
  message: ReactNode;
  destructive?: boolean;
  execute: () => Promise<void>;
}

function KeyHistoryPanel({ publisher, disabled, onConfirm }: {
  publisher: PublisherRecord;
  disabled: boolean;
  onConfirm: (confirmation: RegistryConfirmation) => void;
}) {
  const [keys, setKeys] = useState<PublisherKeyHistory[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError(null);
    publisherApi.listKeys(publisher.id)
      .then(value => { if (current) setKeys(value); })
      .catch(e => { if (current) setError(String(e)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [publisher.id, publisher.key_id]);

  const ts = (t: number) => new Date(t * 1000).toLocaleDateString();

  return (
    <div className="w-[100cqw] space-y-2 border-t border-gray-100 bg-gray-50 p-4">
      <h3 className="break-all text-sm font-semibold text-gray-800">Key History — {publisher.name}</h3>
      <p className="text-xs text-gray-500">Retired keys still verify old packages until manually revoked.</p>
      {loading && <div className="text-xs text-gray-400">Loading...</div>}
      {error && <p role="alert" className="text-xs text-red-600">{error}</p>}
      {!loading && keys !== null && keys.length === 0 && (
        <div className="text-xs text-gray-400">No historical keys.</div>
      )}
      {!loading && keys && keys.length > 0 && (
        <ManagementTable label="Key history" columns={["Key ID", "Status", "Active from", "Retired at", "Action"]}>
          {keys.map(key => (
            <tr key={key.id} className={managementRow}>
              <td className={managementCell + " font-mono text-xs"}>{key.key_id}</td>
              <td className={managementCell}>
                <ManagementStatus tone={key.status === "retired" ? "neutral" : "danger"}>
                  {key.status === "retired" ? "Retired" : "Revoked"}
                </ManagementStatus>
              </td>
              <td className={managementCell}>{ts(key.created_at)}</td>
              <td className={managementCell}>{ts(key.retired_at)}</td>
              <td className={managementCell}>
                {key.status === "retired" && (
                  <button
                    type="button"
                    disabled={disabled}
                    className={managementButton.danger}
                    onClick={() => onConfirm({
                      title: "Revoke historical key",
                      confirmLabel: "Revoke",
                      message: <>Revoke <code className="break-all">{key.key_id}</code>? Packages signed with this key will fail verification immediately.</>,
                      destructive: true,
                      execute: async () => {
                        await publisherApi.revokeHistoricalKey(key.id);
                        setKeys(previous => previous?.map(entry => entry.id === key.id ? { ...entry, status: "revoked" } : entry) ?? null);
                      },
                    })}
                  >
                    <ShieldOff size={14} aria-hidden="true" /> Revoke
                  </button>
                )}
              </td>
            </tr>
          ))}
        </ManagementTable>
      )}
    </div>
  );
}

// ── Registration ──────────────────────────────────────────────────

function HowItWorks() {
  return (
    <div className="bg-white rounded-xl border border-gray-200 p-4 text-sm text-gray-600 space-y-2">
      <h2 className="font-semibold text-gray-800">How publishers register</h2>
      <p>
        A publisher signs a Publisher Profile and serves it as the <code className="bg-gray-100 px-1 rounded">publisher</code> member
        of <code className="bg-gray-100 px-1 rounded">{'https://<publisher>/.well-known/actr'}</code>. It then sends a
        <code className="bg-gray-100 px-1 mx-1 rounded">register_publisher</code> request, signed by an active Publisher Profile
        signing key, to this Actr Facility Provider&apos;s <code className="bg-gray-100 px-1 rounded">realm-management</code> endpoint.
      </p>
      <p>
        The registry pins the profile-signing keys from the first accepted profile. Package-signing keys are separate
        and verify package archives. An update must advance the profile sequence and preserve continuity with the
        pinned keys. If the publisher changes its signing identity without that continuity, an admin has to confirm it again.
      </p>
      <p>
        A replaced package key can still verify older archives while it remains listed as retired and the publisher
        is active. Removing the key from a later accepted profile or revoking it here ends that trust.
      </p>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────

export function PublisherRegistryPage() {
  const [detail, setDetail] = useState<ServiceDetail | null>(null);
  const [detailError, setDetailError] = useState("");
  useEffect(() => {
    api.getServiceDetail("publishers").then(setDetail).catch(e => setDetailError(e.message));
  }, []);
  const [publishers, setPublishers] = useState<PublisherRecord[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);
  const actionBusy = useRef(false);
  const [confirmation, setConfirmation] = useState<RegistryConfirmation | null>(null);
  const selectedPublisher = publishers.find(publisher => publisher.id === selectedId);

  const loadData = useCallback(async () => {
    try {
      setPublishers(await publisherApi.list());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void loadData(); }, [loadData]);

  async function runAction(execute: () => Promise<void>) {
    if (actionBusy.current) return;
    actionBusy.current = true;
    setActionLoading(true);
    try {
      await execute();
      await loadData();
    } catch (error) {
      setError(String(error));
    } finally {
      setConfirmation(null);
      actionBusy.current = false;
      setActionLoading(false);
    }
  }

  const stats = {
    total: publishers.length,
    active: publishers.filter(m => m.status === 'active').length,
    suspended: publishers.filter(m => m.status === 'suspended').length,
  };

  return (
    <ServicePageLayout title="Publisher Registry" description="Review registered publishers and their signing keys.">
      {loading ? <p role="status" className="text-sm text-gray-600">Loading publishers…</p> : <>
        {detail ? <ServiceAccessCard service="publishers" data={detail} publisher={selectedPublisher?.name} /> :
          <p role="status" className="text-sm text-gray-600">{detailError || "Loading service access…"}</p>}

        <HowItWorks />

        {error && (
          <div role="alert" className="bg-red-50 border border-red-200 rounded-lg p-3 text-sm text-red-700">{error}</div>
        )}

        {/* Stats */}
        <div className="grid gap-3 sm:grid-cols-3">
          {[
            { label: 'Total', value: stats.total, color: 'text-gray-700' },
            { label: 'Active', value: stats.active, color: 'text-green-700' },
            { label: 'Suspended', value: stats.suspended, color: 'text-orange-700' },
          ].map(s => (
            <div key={s.label} className={managementCard + " p-4"}>
              <div className={`text-2xl font-bold ${s.color}`}>{s.value}</div>
              <div className="text-gray-500 text-sm">{s.label}</div>
            </div>
          ))}
        </div>

        {/* Publisher Registry Table */}
        <div className={managementCard + " overflow-hidden"}>
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-gray-100 px-4 py-3">
            <h2 className="font-semibold text-gray-800">Publishers</h2>
            {selectedPublisher && (
              <button type="button" onClick={() => setSelectedId(null)} className={managementButton.secondary}>
                Close key history
              </button>
            )}
          </div>
          <ManagementTable label="Publishers" columns={["Name", "Status", "Key ID", "Key Expires", "Actions"]}>
              {publishers.length === 0 && (
                <tr><td colSpan={5} className="px-4 py-8 text-center text-gray-400">No publishers registered</td></tr>
              )}
              {publishers.map(publisher => {
                const isSelected = selectedId === publisher.id;
                const historyId = "publisher-key-history-" + publisher.id;
                const toggleHistory = () => setSelectedId(isSelected ? null : publisher.id);
                return (
                  <Fragment key={publisher.id}>
                    <tr
                      className={managementRow + " cursor-pointer hover:bg-gray-50" + (isSelected ? " bg-blue-50" : "")}
                      onClick={toggleHistory}
                    >
                      <td className={managementCell}>
                        <ManagementDisclosure
                          expanded={isSelected}
                          controls={historyId}
                          onClick={(event) => { event.stopPropagation(); toggleHistory(); }}
                        >
                          <span className="whitespace-nowrap font-mono">{publisher.name}</span>
                        </ManagementDisclosure>
                      </td>
                      <td className={managementCell}><StatusBadge status={publisher.status} /></td>
                      <td className={managementCell + " font-mono text-xs text-gray-600"}>{publisher.key_id || "—"}</td>
                      <td className={managementCell}>
                        <KeyExpiryBadge expiresAt={publisher.key_expires_at} />
                      </td>
                      <td className={managementCell} onClick={e => e.stopPropagation()}>
                        <div className="flex w-max gap-2">
                          {publisher.status === 'active' && (
                            <button
                              type="button"
                              onClick={() => setConfirmation({
                                title: "Suspend publisher",
                                confirmLabel: "Suspend",
                                message: <>Suspend <strong className="break-all">{publisher.name}</strong>? Key lookup stops while the publisher is suspended. You can reinstate it later.</>,
                                execute: () => publisherApi.suspend(publisher.id),
                              })}
                              disabled={actionLoading}
                              className={managementButton.secondary}
                            >Suspend</button>
                          )}
                          {publisher.status === 'active' && publisher.key_id && (
                            <button
                              type="button"
                              onClick={() => setConfirmation({
                                title: "Revoke current key",
                                confirmLabel: "Revoke key",
                                message: <>Revoke <code className="break-all">{publisher.key_id}</code> for <strong className="break-all">{publisher.name}</strong>? Packages signed with this key will fail verification immediately. Any staged replacement key will also be revoked. The publisher must register a fresh key to restore verification.</>,
                                destructive: true,
                                execute: () => publisherApi.revokeActiveKey(publisher.id),
                              })}
                              disabled={actionLoading}
                              title="Emergency revoke of the current package signing key"
                              className={managementButton.danger}
                            ><ShieldOff size={14} aria-hidden="true" /> Revoke key</button>
                          )}
                          {publisher.status === 'suspended' && (
                            <button
                              type="button"
                              onClick={() => void runAction(() => publisherApi.reinstate(publisher.id))}
                              disabled={actionLoading}
                              className={managementButton.primary}
                            >Reinstate</button>
                          )}
                          <button
                            type="button"
                            onClick={() => setConfirmation({
                              title: "Delete publisher",
                              confirmLabel: "Delete",
                              message: <>Permanently delete <strong className="break-all">{publisher.name}</strong> and its signing-key history?</>,
                              destructive: true,
                              execute: () => publisherApi.delete(publisher.id),
                            })}
                            disabled={actionLoading}
                            className={managementButton.danger}
                          >Delete</button>
                        </div>
                      </td>
                    </tr>
                    <tr id={historyId} hidden={!isSelected}>
                      <td colSpan={5} className="p-0">
                        {isSelected && <KeyHistoryPanel publisher={publisher} disabled={actionLoading} onConfirm={setConfirmation} />}
                      </td>
                    </tr>
                  </Fragment>
                );
              })}
          </ManagementTable>
        </div>

        <IdentityReview kind="publishers" />
      </>}
      {confirmation && (
        <ConfirmModal
          isOpen
          title={confirmation.title}
          message={confirmation.message}
          confirmLabel={confirmation.confirmLabel}
          isDestructive={confirmation.destructive}
          loading={actionLoading}
          onConfirm={() => void runAction(confirmation.execute)}
          onCancel={() => { if (!actionBusy.current) setConfirmation(null); }}
        />
      )}
    </ServicePageLayout>
  );
}
