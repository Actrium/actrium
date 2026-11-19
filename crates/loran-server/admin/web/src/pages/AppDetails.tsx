import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  api,
  type ActionResult,
  type AdminCapabilities,
  type AppProfile,
  type ManagementClient,
  type MemberAction,
  type RealmInfo,
  type RealmMembersResponse,
} from "../lib/api";
import { copyText, operationId } from "../lib/utils";
import { appStatus } from "../components/realms/RealmTable";
import { SignedAppRequest } from "../components/realms/SignedAppRequest";
import { AppProfileEditor } from "../components/realms/AppProfileEditor";
import { OwnerInviteModal } from "../components/realms/OwnerInviteModal";
import { ConfirmModal } from "../components/ui/ConfirmModal";
import { ServicePageLayout } from "../components/layout/ServicePageLayout";

const section = "space-y-3 rounded-xl border border-gray-200 bg-white p-5";
const button = "rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm disabled:opacity-50 hover:bg-gray-50";
const field = "rounded-lg border border-gray-300 px-3 py-2 text-sm";
interface Details {
  app: RealmInfo;
  profile: AppProfile | null;
  members: RealmMembersResponse;
  device: ManagementClient;
  capabilities: AdminCapabilities;
}
interface Confirmation {
  label: string;
  message: string;
  execute: () => Promise<void>;
}

export function AppDetails() {
  const { realm = "" } = useParams();
  return <DetailsPage key={realm} id={realm} />;
}

function DetailsPage({ id }: { id: string }) {
  const [data, setData] = useState<Details | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [code, setCode] = useState("");
  const [purpose, setPurpose] = useState<"viewer" | "editor" | "owner_device">("viewer");
  const [days, setDays] = useState(1);
  const [issued, setIssued] = useState<ActionResult | null>(null);
  const [ownerCode, setOwnerCode] = useState<ActionResult | null>(null);
  const [confirm, setConfirm] = useState<Confirmation | null>(null);
  const receiptKey = "loran-admin:invite:" + id;
  const [lastRequest, setLastRequest] = useState(() => sessionStorage.getItem(receiptKey));
  const refresh = useCallback(async () => {
    const [app, profile, members, device, capabilities] = await Promise.all([
      api.getRealm(id),
      api.appProfile(id),
      api.listRealmMembers(id),
      api.managementClient(id),
      api.getCapabilities(),
    ]);
    if (!app.realm) throw Error("App not found");
    setData({ app: app.realm, profile: profile.profile, members, device, capabilities });
  }, [id]);
  useEffect(() => {
    void refresh().catch((e) => setError(e.message));
  }, [refresh]);
  useEffect(() => {
    if (!issued?.invite) return;
    const timer = setTimeout(() => setIssued(null), 120_000);
    return () => clearTimeout(timer);
  }, [issued]);
  async function run(job: () => Promise<void>) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await job();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Operation failed");
    } finally {
      try {
        await refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "Refresh failed");
      }
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function memberAction(action: MemberAction) {
    const key = operationId();
    const issuing = action.action === "invite_member" || action.action === "invite_owner_device";
    if (issuing) {
      sessionStorage.setItem(receiptKey, key);
      setLastRequest(key);
      setIssued(null);
    }
    const result = await api.memberAction(id, key, action);
    if (issuing) setIssued(result);
  }
  function ask(label: string, message: string, execute: () => Promise<void>) {
    setConfirm({ label, message, execute });
  }
  const writable = data?.capabilities.realm_writes_enabled && !busy;
  const active = data?.app.enabled && data?.app.status === "active";
  const manage = Boolean(writable && active && data?.device.connected);
  const ownDomain = data && id.split("@")[1] === data.capabilities.provider_id;
  return (
    <ServicePageLayout
      title={data?.profile?.realm.display_name || id}
      description={
        <span>
          App ID <code>{id}</code>
        </span>
      }
      headerActions={
        <Link className="text-sm text-blue-700" to="/admin/apps">
          All apps
        </Link>
      }
    >
      {error && (
        <p role="alert" className="rounded-lg bg-red-50 p-3 text-sm text-red-700">
          {error}
        </p>
      )}
      {!data ? (
        <p>{error ? "App details unavailable." : "Loading app…"}</p>
      ) : (
        <>
          <section className={section}>
            <h2 className="font-semibold">Application</h2>
            <p className="text-sm">
              App URI · <code>{"actr://" + id.split("@")[1] + "/" + id.split("@")[0]}</code>
            </p>
            <p className="text-sm">Status · {appStatus(data.app)}</p>
            {data.capabilities.superv_managed && <p>superv manages changes on this node.</p>}
            {data.app.status !== "retired" && (
              <button
                className={button}
                disabled={!writable}
                onClick={() =>
                  void run(async () => {
                    await api.updateRealm(id, { enabled: !data.app.enabled });
                  })
                }
              >
                {data.app.enabled ? "Suspend app" : "Resume app"}
              </button>
            )}
          </section>
          <section className={section}>
            <h2 className="font-semibold">Owner management device</h2>
            <p className="text-sm">
              {data.device.connected
                ? "Authorized by the current Owner"
                : data.device.available
                  ? "Ask the Owner to issue an add-device invite for their own membership."
                  : "Owner management requires the Issuer and Signer services."}
            </p>
            {!data.device.connected && data.device.available && (
              <p className="text-xs text-gray-600">
                From the Owner’s client, run{" "}
                <code>actr realm invite --key OWNER_KEY_FILE --member OWNER_MEMBER_ID</code> and paste the
                resulting invite here.
              </p>
            )}
            {data.device.connected ? (
              <code className="text-xs">{data.device.key_id}</code>
            ) : (
              data.device.available && (
                <form
                  className="flex flex-wrap gap-2"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void run(async () => {
                      await api.connectManagementClient(id, code.trim());
                      setCode("");
                    });
                  }}
                >
                  <input
                    aria-label="Owner device invite"
                    type="password"
                    autoComplete="off"
                    required
                    value={code}
                    onChange={(e) => setCode(e.target.value)}
                    className={field}
                    placeholder="Owner device invite"
                  />
                  <button disabled={!writable || !active} className={button}>
                    Authorize Admin
                  </button>
                </form>
              )
            )}
          </section>
          <section className={section}>
            <h2 className="font-semibold">Invitations</h2>
            <form
              className="flex flex-wrap items-end gap-3"
              onSubmit={(e) => {
                e.preventDefault();
                void run(() =>
                  memberAction(
                    purpose === "owner_device"
                      ? { action: "invite_owner_device", days }
                      : { action: "invite_member", role: purpose, days },
                  ),
                );
              }}
            >
              <label className="grid gap-1 text-sm">
                Invite for
                <select
                  className={field}
                  value={purpose}
                  onChange={(e) => setPurpose(e.target.value as typeof purpose)}
                >
                  <option value="viewer">New Viewer</option>
                  <option value="editor">New Editor</option>
                  <option value="owner_device">Additional Owner device</option>
                </select>
              </label>
              <label className="grid gap-1 text-sm">
                Days
                <input
                  type="number"
                  className={field + " w-20"}
                  required
                  min={1}
                  max={30}
                  value={days || ""}
                  onChange={(e) => setDays(Number(e.target.value))}
                />
              </label>
              <button disabled={!manage} className={button}>
                Generate invite
              </button>
            </form>
            {lastRequest && !issued?.invite && manage && (
              <button
                className={button}
                onClick={() =>
                  void run(async () => {
                    setIssued(await api.revealInvitation(id, lastRequest));
                  })
                }
              >
                Retrieve last invite
              </button>
            )}
            {issued?.invite && (
              <div className="space-y-2 rounded-lg bg-blue-50 p-3">
                <label className="grid gap-1 text-sm">
                  Invitation code
                  <textarea readOnly value={issued.invite} className={field + " font-mono text-xs"} />
                </label>
                <div className="flex gap-2">
                  <button
                    className={button}
                    onClick={() =>
                      void copyText(issued.invite!).catch(() =>
                        setError("Select the invitation code and copy it."),
                      )
                    }
                  >
                    Copy code
                  </button>
                  <button
                    className={button}
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        if (lastRequest) await api.dismissInvitation(id, lastRequest);
                        setIssued(null);
                        sessionStorage.removeItem(receiptKey);
                        setLastRequest(null);
                      })
                    }
                  >
                    Done
                  </button>
                </div>
              </div>
            )}
            <h3 className="text-sm font-medium">Open invitations ({data.members.invites.length})</h3>
            <ul className="divide-y divide-gray-100">
              {data.members.invites.map((i) => (
                <li key={i.invite_id} className="flex flex-wrap items-center justify-between gap-2 py-2 text-sm">
                  <span>
                    <code>{i.invite_id}</code> ·{" "}
                    {i.role === "owner" ? "First Owner / recovery" : i.role || "Additional device"} · Expires{" "}
                    {new Date(i.expires_at).toLocaleString()}
                  </span>
                  <button
                    className={button}
                    disabled={!manage}
                    onClick={() =>
                      ask("Revoke invitation", i.invite_id, () =>
                        memberAction({ action: "revoke_invite", invite_id: i.invite_id }),
                      )
                    }
                  >
                    Revoke invitation
                  </button>
                </li>
              ))}
            </ul>
          </section>
          <section className={section}>
            <h2 className="font-semibold">Members and devices</h2>
            {!data.members.members.length && (
              <p className="text-sm">The first Owner invite is awaiting redemption.</p>
            )}
            {data.members.members.map((m) => (
              <div className="space-y-2 border-t border-gray-100 pt-3 text-sm" key={m.member_id}>
                <p>
                  <strong>{m.role}</strong> · <code>{m.member_id}</code> · {m.status}
                </p>
                {m.status === "active" && m.role !== "owner" && (
                  <div className="flex flex-wrap gap-2">
                    <button
                      className={button}
                      disabled={!manage}
                      onClick={() =>
                        ask("Change role", m.member_id, () =>
                          memberAction({
                            action: "set_role",
                            member_id: m.member_id,
                            role: m.role === "viewer" ? "editor" : "viewer",
                          }),
                        )
                      }
                    >
                      Make {m.role === "viewer" ? "Editor" : "Viewer"}
                    </button>
                    <button
                      className={button}
                      disabled={!manage}
                      onClick={() =>
                        ask(
                          "Transfer ownership",
                          "The selected member becomes Owner. This Admin device loses Owner access.",
                          () => memberAction({ action: "transfer_owner", member_id: m.member_id }),
                        )
                      }
                    >
                      Transfer ownership
                    </button>
                    <button
                      className={button}
                      disabled={!manage}
                      onClick={() =>
                        ask("Remove member", "All devices of " + m.member_id + " will be revoked.", () =>
                          memberAction({ action: "remove_member", member_id: m.member_id }),
                        )
                      }
                    >
                      Remove member
                    </button>
                  </div>
                )}
                {m.clients.map((c) => (
                  <div key={c.key_id} className="flex items-center justify-between gap-2 pl-3 text-xs">
                    <span>
                      {c.label || "Device"} · <code>{c.key_id}</code>
                      {c.revoked_at && " · Revoked"}
                    </span>
                    {!c.revoked_at && (
                      <button
                        disabled={!manage}
                        className={button}
                        onClick={() =>
                          ask(
                            "Revoke device",
                            "Unused invites signed by " + c.key_id + " will also be revoked.",
                            () => memberAction({ action: "revoke_client", key_id: c.key_id }),
                          )
                        }
                      >
                        Revoke device
                      </button>
                    )}
                  </div>
                ))}
              </div>
            ))}
          </section>
          <section className={section}>
            <h2 className="font-semibold">Realm profile</h2>
            {data.profile ? (
              <>
                <p className="text-sm">
                  Provider {data.profile.provider.id} · Sequence {data.profile.sequence}
                </p>
                <p className="text-sm">Issuer {data.profile.issuer?.url || "—"}</p>
                {data.profile.issuer?.keys?.length ? (
                  <p className="text-sm">
                    Issuer verification keys ·{" "}
                    {data.profile.issuer.keys.map((k) => k.id + " (" + k.status + ")").join(", ")}
                  </p>
                ) : null}
                {Object.entries(data.profile.packages || {}).map(([ref, p]) => (
                  <details className="break-all text-sm" key={ref}>
                    <summary className="cursor-pointer">
                      {ref}{" "}
                      {ref === data.profile?.primary_package
                        ? "(Service)"
                        : ref === data.profile?.front_package
                          ? "(Front)"
                          : ""}{" "}
                      · <code>{p.type}</code>
                    </summary>
                    <dl className="mt-2 grid gap-1 pl-4 text-gray-600">
                      <dt>Archive</dt>
                      <dd>
                        <a className="text-blue-700" href={p.href} rel="noreferrer" target="_blank">
                          {p.href}
                        </a>
                      </dd>
                      <dt>SHA-256</dt>
                      <dd>
                        <code>{p.sha256}</code>
                      </dd>
                      {Object.keys(p.dependencies || {}).length > 0 && (
                        <>
                          <dt>Dependencies</dt>
                          <dd>
                            {Object.entries(p.dependencies || {})
                              .map(([alias, target]) => alias + " → " + target)
                              .join(", ")}
                          </dd>
                        </>
                      )}
                    </dl>
                  </details>
                ))}
                {data.profile.package && !data.profile.packages && (
                  <p>Legacy package · {data.profile.package.type}</p>
                )}
                {ownDomain && (
                  <AppProfileEditor
                    key={data.profile.sequence}
                    profile={data.profile}
                    disabled={!manage}
                    onSave={(change) => run(() => memberAction(change))}
                  />
                )}
              </>
            ) : (
              <p className="text-sm">This app has no signed Realm Profile.</p>
            )}
          </section>
          {!ownDomain && (
            <SignedAppRequest
              appId={id}
              disabled={!writable}
              onAccepted={() => void refresh().catch((e) => setError(e.message))}
            />
          )}
          {data.app.status !== "retired" && (
            <section className={section}>
              <h2 className="font-semibold text-red-800">Recovery and retirement</h2>
              {ownDomain ? (
                <div className="flex gap-2">
                  <button
                    className={button}
                    disabled={!writable || data.app.status !== "active"}
                    onClick={() =>
                      ask(
                        "Recover Owner",
                        "Issuing invalidates older unused Owner recovery codes. Redeeming the new code revokes all existing Owner devices.",
                        async () => {
                          const r = await api.issueOwnerInvite(id);
                          setOwnerCode({ invite: r.owner_invite, expires_at: r.expires_at });
                        },
                      )
                    }
                  >
                    Recover Owner
                  </button>
                  <button
                    className={button}
                    disabled={!writable}
                    onClick={() =>
                      ask("Retire app", "Retirement is permanent. The App ID cannot be reused.", async () => {
                        await api.retireRealm(id);
                      })
                    }
                  >
                    Retire app
                  </button>
                </div>
              ) : (
                <p className="text-sm">The external domain holder signs recovery and retirement for this app.</p>
              )}
            </section>
          )}
        </>
      )}
      {confirm && (
        <ConfirmModal
          isOpen
          title={confirm.label}
          message={confirm.message}
          confirmLabel={confirm.label}
          cancelLabel="Cancel"
          isDestructive
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const job = confirm.execute;
            setConfirm(null);
            void run(job);
          }}
        />
      )}
      {ownerCode?.invite && (
        <OwnerInviteModal
          realm={id}
          invite={ownerCode.invite}
          expiresAt={ownerCode.expires_at}
          onClose={() => setOwnerCode(null)}
        />
      )}
    </ServicePageLayout>
  );
}
