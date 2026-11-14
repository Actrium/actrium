import { useEffect, useState } from "react";
import { parse, stringify } from "smol-toml";
import { api, type ResolvedField } from "../../lib/api";

export function DevelopmentJoinSettings({
  fields,
  writable,
  onRefresh,
}: {
  fields: ResolvedField[];
  writable: boolean;
  onRefresh: () => Promise<void>;
}) {
  const currentMode = fields.find((f) => f.key === "realm.dev_join.mode")?.effective_value ?? "off";
  const currentRole = fields.find((f) => f.key === "realm.dev_join.default_role")?.effective_value ?? "editor";
  const [mode, setMode] = useState(currentMode);
  const [role, setRole] = useState(currentRole);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => { setMode(currentMode); setRole(currentRole); }, [currentMode, currentRole]);

  async function save() {
    if (!writable) return;
    setSaving(true);
    setMessage("");
    setError("");
    try {
      const { content } = await api.getConfigFile();
      const document = parse(content);
      document.realm = { ...(document.realm as object), dev_join: { mode, default_role: role } };
      await api.saveConfigFile(stringify(document));
      const result = await api.reloadNode();
      if (!result.accepted) throw new Error("Configuration reload was refused.");
      // Confirm the running router has adopted the saved policy.
      for (let attempt = 0; attempt < 20; attempt++) {
        await new Promise((resolve) => setTimeout(resolve, 500));
        const detail = await api.getServiceDetail("issuer");
        const running = detail.config?.dev_join as { mode?: string; default_role?: string } | undefined;
        if (running?.mode === mode && running?.default_role === role) {
          await onRefresh();
          setMessage("Development admission updated.");
          return;
        }
      }
      throw new Error("Configuration saved, but reload could not be confirmed. Check the server logs.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save configuration.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-5 space-y-4">
      <h2 className="text-sm font-semibold text-gray-900">Development admission</h2>
      <p className="text-sm text-gray-500">
        Applies to every active Realm on this AFP. Open mode lets anyone join. Missing or invalid
        invitations use the default role; valid invitations retain normal redemption.
      </p>
      {!writable && <p className="text-sm text-amber-700" role="status">
        Supervisor-managed AFP: local admission settings are read-only.
      </p>}
      <div className="flex flex-wrap items-end gap-4">
        <label className="text-sm text-gray-700">
          Mode
          <select value={mode} onChange={(e) => setMode(e.target.value)} disabled={!writable || saving}
            className="mt-1 block rounded-md border border-gray-300 px-3 py-2">
            <option value="off">Off — one-time invitations</option>
            <option value="open">Open — any or no invitation</option>
            <option value="reusable_invite">Reusable invitations</option>
          </select>
        </label>
        <label className="text-sm text-gray-700">
          New member role
          <select value={role} onChange={(e) => setRole(e.target.value)} disabled={!writable || saving || mode !== "open"}
            className="mt-1 block rounded-md border border-gray-300 px-3 py-2 disabled:opacity-50">
            <option value="editor">Editor</option>
            <option value="viewer">Viewer</option>
          </select>
        </label>
        <button onClick={save} disabled={!writable || saving || (mode === currentMode && role === currentRole)}
          className="rounded-md bg-blue-600 px-4 py-2 text-sm text-white disabled:opacity-50">
          {saving ? "Applying…" : "Save and apply"}
        </button>
      </div>
      {mode === "reusable_invite" && <p className="text-sm text-gray-500">
        Unused member and client invitations remain reusable until they expire or are revoked.
        Owner recovery invitations remain single-use.
      </p>}
      {message && <p role="status" className="text-sm text-green-700">{message}</p>}
      {error && <p role="alert" className="text-sm text-red-700">{error}</p>}
    </section>
  );
}
