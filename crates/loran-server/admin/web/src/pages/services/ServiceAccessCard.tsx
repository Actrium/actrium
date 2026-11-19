import { useEffect, useId, useState, type ReactNode } from "react";
import { Copy, Pencil } from "lucide-react";
import { ConfigFieldModal } from "../../components/ui/ConfigFieldModal";
import { api, type AppProfile, type RealmInfo, type ResolvedField, type ServiceDetail } from "../../lib/api";
import { copyText } from "../../lib/utils";

const callers = {
  stun: "ICE clients discovering their mapped address",
  turn: "ICE clients requesting a media or data relay",
  signaling: "Actrium clients and services exchanging WebRTC signaling",
  relay: "Endpoints carrying encrypted application channels",
  publishers: "AFP administrators and package verifiers",
  issuer: "App clients requesting membership and Actor credentials",
  signer: "Internal gRPC clients authorized to request signatures",
};
type Service = keyof typeof callers;
type ProfileService = "stun" | "turn" | "signaling" | "issuer";

function Address({ value, label }: { value: string; label: string }) {
  const [result, setResult] = useState("");
  useEffect(() => {
    if (!result) return;
    const timer = setTimeout(() => setResult(""), 2400);
    return () => clearTimeout(timer);
  }, [result]);
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <code className="break-all select-all text-sm text-gray-900">{value}</code>
      <button type="button" aria-label={`Copy ${label}`} title={`Copy ${label}`}
        className="rounded p-1 text-gray-500 hover:bg-blue-50 hover:text-blue-700"
        onClick={() => void copyText(value).then(() => setResult("Copied"), () => setResult("Select the address to copy it."))}>
        <Copy size={14} />
      </button>
      <span role="status" className="text-xs text-gray-600">{result}</span>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return <div className="min-w-0"><dt className="text-xs text-gray-500">{label}</dt>
    <dd className="mt-1 break-words text-sm text-gray-900">{children}</dd></div>;
}

const TURN_REALM = "turn.realm";
const ISSUER_TTL = "services.issuer.server.token_ttl_secs";
const ISSUER_HEARTBEAT = "services.issuer.server.signaling_heartbeat_interval_secs";
const inlineSettings: Record<string, { label: string; suffix: string }> = {
  [TURN_REALM]: { label: "TURN authentication realm", suffix: " (TURN authentication domain)" },
  [ISSUER_TTL]: { label: "Actor and TURN credential lifetime", suffix: " s" },
  [ISSUER_HEARTBEAT]: { label: "Signaling heartbeat interval", suffix: " s" },
};
const settingPrefixes: Partial<Record<Service, string>> = {
  turn: "turn.",
  issuer: "services.issuer.server.",
  signaling: "services.signaling.server.",
};

type SettingValue = string | number | boolean;

function runningSetting(data: ServiceDetail, prefix: string | undefined, key: string): SettingValue | undefined {
  if (!prefix || !key.startsWith(prefix)) return;
  let value: unknown = data.config;
  for (const segment of key.slice(prefix.length).split(".")) {
    if (value === null || typeof value !== "object") return;
    value = (value as Record<string, unknown>)[segment];
  }
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return value;
}

function settingDiffers(value: SettingValue | undefined, field: ResolvedField): boolean {
  if (value === undefined) return false;
  return typeof value === "number"
    ? value !== Number(field.effective_value)
    : String(value) !== field.effective_value;
}

function SettingField({ label, field, value, suffix = "", onEdit }: {
  label: string;
  field?: ResolvedField;
  value?: SettingValue;
  suffix?: string;
  onEdit: (field: ResolvedField) => void;
}) {
  const displayed = value ?? field?.effective_value;
  const editLabel = `Edit ${label}`;
  return (
    <Field label={label}>
      <div className="flex flex-wrap items-center gap-1.5">
        <span>{displayed === undefined ? "Unavailable" : String(displayed) + suffix}</span>
        {field && (
          <button
            type="button"
            aria-label={editLabel}
            title={editLabel}
            className="inline-flex rounded border border-gray-200 p-1 text-gray-600 hover:border-blue-200 hover:bg-blue-50 hover:text-blue-700"
            onClick={() => onEdit(field)}
          >
            <Pencil size={12} aria-hidden="true" />
          </button>
        )}
      </div>
      {field && settingDiffers(value, field) && (
        <p className="mt-1 text-xs text-amber-800">Configured value {field.effective_value}{suffix}</p>
      )}
    </Field>
  );
}

function isLoopback(host: string) {
  const h = host.toLowerCase().replace(/\.$/, "");
  return h === "localhost" || h.endsWith(".localhost") || /^127\./.test(h) || h === "[::1]";
}

function addressScope(host: string) {
  if (isLoopback(host)) return "Loopback";
  if (host === "0.0.0.0" || host === "[::]") return "Wildcard bind address";
  if (/^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|\[f[cd])/i.test(host)) return "Private network address";
  if (/^(169\.254\.|\[fe[89ab])/i.test(host)) return "Link-local address";
  return "Configured address · reachability untested";
}

function profileUrls(profile: AppProfile, service: ProfileService): string[] {
  if (service === "issuer") return profile.issuer?.url ? [profile.issuer.url] : [];
  if (service === "signaling") return profile.provider.signaling?.map(s => s.url) ?? [];
  return profile.provider.ice?.flatMap(s => s.urls).filter(url => url.startsWith(service + ":") || url.startsWith(service + "s:")) ?? [];
}

// Fetch only the selected App's published document. Older profiles can lack an endpoint.
function PublishedAddresses({ service }: { service: ProfileService }) {
  const appSelectId = useId();
  const [apps, setApps] = useState<RealmInfo[]>([]);
  const [app, setApp] = useState("");
  const [error, setError] = useState("");
  const [profile, setProfile] = useState<AppProfile | null | undefined>();
  useEffect(() => {
    let active = true;
    api.listRealms().then(r => { if (active) setApps(r.realms); }, e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, []);
  useEffect(() => {
    if (!app) return;
    let active = true;
    api.appProfile(app).then(r => { if (active) setProfile(r.profile); }, e => { if (active) setError(e.message); });
    return () => { active = false; };
  }, [app]);
  const urls = profile ? profileUrls(profile, service) : [];
  return <div className="mt-3 space-y-2">
    <div className="flex flex-wrap items-center gap-2">
      <label htmlFor={appSelectId} className="text-xs text-gray-600">App</label>
      <select id={appSelectId} className="max-w-full rounded border border-gray-200 px-2 py-1 text-sm" value={app}
        onChange={e => { setProfile(undefined); setError(""); setApp(e.target.value); }}>
        <option value="">Select an App</option>
        {apps.map(a => <option key={a.realm} value={a.realm}>{a.realm}</option>)}
      </select>
    </div>
    {error ? <p role="alert">{error}</p> : app && (profile === undefined ? "Loading profile…" :
      profile === null ? "No published Realm Profile." : urls.length ? <>
        <p className="text-xs text-gray-500">Realm Profile · revision {profile.sequence}</p>
        {urls.map((url, i) => <Address key={i} value={url} label="published endpoint" />)}
      </> : "This Realm Profile has no endpoint for this service.")}
  </div>;
}

const issuerRoutes = [
  ["Membership", "membership", "signed membership request"],
  ["Register and issue credentials", "register", "registered App client proof"],
  ["Renew credentials", "renew", "renewal token and client proof"],
  ["Reissue credentials", "reissue", "registered client proof"],
  ["Issue relay identity", "relay-identity", "renewal token and client proof"],
] as const;

export function ServiceAccessCard({ service, data, publisher, onRefresh }: {
  service: Service;
  data: ServiceDetail;
  publisher?: string;
  onRefresh?: () => void;
}) {
  const [compare, setCompare] = useState(false);
  const [editField, setEditField] = useState<ResolvedField | null>(null);
  const [reloading, setReloading] = useState(false);
  const [reloadMessage, setReloadMessage] = useState("");
  const access = data.access;
  const settings = (data.config_fields ?? []).filter(field => field.dynamic);
  const externalRelay = service === "relay" && !data.enabled && !!access?.url && access.source === "relay.public_url";
  const hasAccess = (data.enabled || externalRelay) && !!access;
  const extraSettings = settings.filter(field => !(hasAccess && inlineSettings[field.key]));
  const reloadPending = settings.some(field =>
    field.reloadable && settingDiffers(runningSetting(data, settingPrefixes[service], field.key), field),
  );

  function renderSetting(key: string) {
    const field = settings.find(candidate => candidate.key === key);
    const presentation = inlineSettings[key];
    return (
      <SettingField
        key={key}
        label={presentation?.label ?? field?.description ?? key}
        suffix={presentation?.suffix}
        field={field}
        value={runningSetting(data, settingPrefixes[service], key)}
        onEdit={setEditField}
      />
    );
  }

  async function reloadConfiguration() {
    setReloading(true);
    setReloadMessage("");
    try {
      await api.reloadNode();
      setReloadMessage("Reload requested. Values update when the node applies the configuration.");
      onRefresh?.();
    } catch (error) {
      setReloadMessage(error instanceof Error ? error.message : "Could not request a reload.");
    } finally {
      setReloading(false);
    }
  }

  const ice = service === "stun" || service === "turn";
  const profileService = ice || service === "issuer" || service === "signaling" ? service : null;
  const url = access?.url;
  let endpoint: URL | undefined;
  if (url) {
    try { endpoint = new URL(ice ? url.replace(":", "://") : url); } catch { /* Display the configured value for diagnosis. */ }
  }
  const scheme = endpoint?.protocol.slice(0, -1);
  const transport = ice ? "UDP" : "TCP";
  const port = endpoint?.port || (scheme === "https" || scheme === "wss" ? "443" : scheme === "http" || scheme === "ws" ? "80" : undefined);
  const protocol = service === "signer" ? `gRPC / ${access?.mtls ? "HTTP/2 with mTLS" : "cleartext HTTP/2"}` : scheme?.toUpperCase();
  const label = service === "publishers" ? "Admin list API · GET" : service === "signer" ? "Internal gRPC endpoint" :
    service === "issuer" ? "Issuer base URL" : externalRelay ? "Published external relay endpoint" : "Client endpoint";
  const turnMode = { static: "Configured long-term users", realm: "Realm-scoped credentials from Issuer", rest: "External REST credential issuer" };
  return <section aria-label="Service access" className="space-y-4 rounded-xl border border-gray-200 bg-white p-5">
    <h2 className="font-semibold text-gray-800">Service access</h2>
    {!data.enabled && <p className="text-sm text-gray-600">
      {service === "relay" ? "Local relay is disabled on this node." : "Disabled on this node."}
    </p>}
    {(data.enabled || externalRelay) && (!access ? <p>Listener configuration unavailable.</p> : <>
      <div className="rounded-lg bg-gray-50 px-3 py-2">
        <p className="mb-1 text-xs font-medium text-gray-600">{label}</p>
        {url && endpoint ? <Address value={url} label={label} /> : <p>{url ? "Invalid configured endpoint" : "No endpoint advertised"} {url && <code>{url}</code>}</p>}
      </div>
      <dl className="grid grid-cols-2 gap-x-5 gap-y-3 lg:grid-cols-4">
        <Field label="Protocol">{protocol ?? "Unavailable"} / {transport}</Field>
        <Field label="Host">{endpoint?.hostname ?? "Unavailable"}</Field>
        <Field label="Port">{port ?? "Unavailable"}</Field>
        {!ice && <Field label={service === "signer" ? "gRPC method path" : "Path"}>
          {service === "signer" ? "/signer.v1.Signer/{method}" : endpoint?.pathname || "Unavailable"}</Field>}
        {endpoint && <Field label="Address scope">{addressScope(endpoint.hostname)}</Field>}
        {data.enabled && <Field label="Local listener">{access.listen} / {transport}</Field>}
        <Field label="Address source">{access.source}</Field>
        <Field label="Used by">{callers[service]}</Field>
        {ice && access.shared && <Field label="Shared listener">STUN and TURN</Field>}
        {service === "turn" && <>
          <Field label="UDP relay port range">{String(data.config?.relay_port_range ?? "Unavailable")}</Field>
          {renderSetting(TURN_REALM)}
          <Field label="Credentials">{access.turn_auth && turnMode[access.turn_auth]}</Field>
          {access.issuer_ttl_secs != null && <Field label="Local Issuer credential lifetime">{access.issuer_ttl_secs} s</Field>}
        </>}
        {service === "relay" && data.enabled && <>
          <Field label="Local WebSocket route">{String(data.config?.path ?? "Unavailable")}</Field>
          <Field label="Authentication">{data.config?.auth === "aid" ? "Issuer-signed relay identity" : String(data.config?.auth ?? "Unavailable")}</Field>
        </>}
        {service === "issuer" && <>
          {renderSetting(ISSUER_TTL)}
          {renderSetting(ISSUER_HEARTBEAT)}
          <Field label="Renewal token lifetime">{access.renewal_ttl_secs ?? "Unavailable"} s</Field>
          <Field label="Relay identity lifetime">Up to {access.relay_identity_max_secs} s, capped by the renewal token expiry</Field>
        </>}
        {service === "signer" && <Field label="Authentication">Signed service requests{access.mtls ? " and mutual TLS" : ""}</Field>}
      </dl>
      {endpoint && isLoopback(endpoint.hostname) && <p className="rounded-lg bg-amber-50 px-3 py-2 text-xs text-amber-900">Loopback address. On another device this address points back to that device.</p>}
      {service === "issuer" && url && <dl className="space-y-2 border-t border-gray-100 pt-3">
        {issuerRoutes.map(([name, route, auth]) => <Field key={route} label={name + " · POST · " + auth}>
          <Address value={url + "/" + route} label={name} /></Field>)}
      </dl>}
      {service === "publishers" && endpoint && <div className="space-y-2 border-t border-gray-100 pt-3">
        <dl><Field label={publisher ? "Public verifying-key lookup · GET · " + publisher : "Public verifying-key lookup · GET · select a publisher below or replace {name}"}>
          <Address value={endpoint.origin + "/registry/publishers/" + (publisher ? encodeURIComponent(publisher) : "{name}") + "/verifying-key"} label="public key lookup" /></Field></dl>
        <dl><Field label="Admin list via this browser’s management origin · GET · admin authentication required">
          <Address value={window.location.origin + endpoint.pathname} label="management origin URL" /></Field></dl>
      </div>}
    </>)}
    {extraSettings.length > 0 && (
      <div className="space-y-3 border-t border-gray-100 pt-4">
        <h3 className="text-sm font-semibold text-gray-800">{service === "signaling" ? "Rate limits" : "Service settings"}</h3>
        <dl className="grid grid-cols-2 gap-x-5 gap-y-3 lg:grid-cols-4">
          {extraSettings.map(field => renderSetting(field.key))}
        </dl>
      </div>
    )}
    {reloadPending && (
      <div className="flex flex-wrap items-center gap-3 rounded-lg bg-amber-50 px-3 py-2 text-sm text-amber-900">
        <span>Reload the node configuration to apply saved settings.</span>
        <button type="button" className="rounded border border-amber-300 bg-white px-2.5 py-1 font-medium hover:bg-amber-100 disabled:opacity-50"
          disabled={reloading} onClick={() => void reloadConfiguration()}>
          {reloading ? "Requesting reload…" : "Reload configuration"}
        </button>
        {reloadMessage && <span role="status" className="text-xs">{reloadMessage}</span>}
      </div>
    )}
    {profileService && <details className="border-t border-gray-100 pt-3 text-sm text-gray-600" onToggle={e => setCompare(e.currentTarget.open)}>
      <summary className="cursor-pointer">Published address by App</summary>
      {compare && <PublishedAddresses service={profileService} />}
    </details>}
    {editField && (
      <ConfigFieldModal field={editField}
        onClose={() => setEditField(null)}
        onSaved={() => {
          setEditField(null);
          setReloadMessage("");
          onRefresh?.();
        }}
      />
    )}
  </section>;
}
