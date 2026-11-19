import { useEffect, useRef, useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { api, type RealmPackageSelection, type RealmPackageState } from "../../lib/api";

const input = "w-full rounded-lg border border-gray-300 px-3 py-2 text-sm font-mono";

export function RealmPackages({ realm, onClose }: { realm: string; onClose: () => void }) {
  const [snapshot, setSnapshot] = useState<RealmPackageState>();
  const [selection, setSelection] = useState<RealmPackageSelection>();
  const [route, setRoute] = useState("");
  const [payload, setPayload] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const [saving, setSaving] = useState(false);
  const pending = useRef<{ key: string; id: string } | undefined>(undefined);

  function accept(state: RealmPackageState) {
    setSnapshot(state);
    setSelection(state.selection);
  }

  useEffect(() => {
    let cancelled = false;
    api.getRealmPackages(realm).then((state) => {
      if (!cancelled) accept(state);
    }).catch((error: Error) => {
      if (!cancelled) setError(error.message);
    });
    return () => { cancelled = true; };
  }, [realm]);

  async function submit(rollback: boolean) {
    const target = rollback ? snapshot?.previous_selection : selection;
    if (!snapshot || !target) return;
    const readiness = route.trim() ? { route: route.trim(), request_base64: payload.trim() || undefined } : undefined;
    const key = JSON.stringify([target, snapshot.sequence, rollback, readiness]);
    if (pending.current?.key !== key) pending.current = { key, id: crypto.randomUUID().replace(/-/g, "") };
    setSaving(true);
    setError("");
    setResult("");
    try {
      const response = await api.updateRealmPackages(realm, {
        selection: target,
        operation: { operation_id: pending.current.id, expected_sequence: snapshot.sequence, rollback },
        readiness,
      });
      accept(await api.getRealmPackages(realm));
      pending.current = undefined;
      setResult(response.changed === false ? "The package selection is already current." : `Published Profile ${response.sequence}.`);
    } catch (error) {
      setError(error instanceof Error ? error.message : "Package update failed.");
    } finally {
      setSaving(false);
    }
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    void submit(false);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <section className="max-h-[92vh] w-full max-w-lg overflow-y-auto rounded-xl bg-white p-6 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-gray-900">Realm packages</h2>
            <p className="text-xs text-gray-500">{realm}{snapshot && ` · Profile ${snapshot.sequence}`}</p>
          </div>
          <button type="button" onClick={onClose} aria-label="Close" className="rounded-lg p-1 text-gray-400 hover:bg-gray-100">
            <X className="h-5 w-5" />
          </button>
        </div>
        {error && <p role="alert" className="my-3 whitespace-pre-wrap text-sm text-red-600">{error}</p>}
        {result && <p role="status" className="my-3 text-sm text-green-700">{result}</p>}
        {selection && snapshot && (
          <form onSubmit={handleSubmit} className="space-y-4">
            {(["primary_package", "front_package"] as const).map((role) => {
              const name = role === "primary_package" ? "Service" : "Front";
              const reference = selection[role];
              const pkg = selection.packages[reference];
              return (
                <fieldset key={role} className="grid gap-1">
                  <legend className="text-sm font-medium text-gray-700">{name} package</legend>
                  {(["href", "sha256", "type"] as const).map((field) => (
                    <input key={field} aria-label={`${name} ${field}`} required value={pkg[field] ?? ""}
                      placeholder={{ href: "HTTPS download URL", sha256: "SHA-256", type: "publisher.example/program@1.0.0" }[field]}
                      pattern={field === "sha256" ? "[0-9a-f]{64}" : undefined} className={input}
                      onChange={(event) => setSelection({ ...selection, packages: { ...selection.packages,
                        [reference]: { ...pkg, [field]: event.target.value.trim() },
                      } })} />
                  ))}
                </fieldset>
              );
            })}
            <details className="space-y-2">
              <summary className="cursor-pointer text-sm font-medium text-gray-700">Service readiness RPC</summary>
              <p className="text-xs text-gray-500">When the Service version changes, Loran probes the target before it publishes.</p>
              <input aria-label="Readiness RPC route" placeholder="echo.v1.EchoService.Echo" maxLength={240}
                value={route} onChange={(event) => setRoute(event.target.value)} className={input} />
              <textarea aria-label="Readiness request body in Base64" placeholder="Base64 request body (optional)"
                value={payload} onChange={(event) => setPayload(event.target.value)} className={input} />
            </details>
            {snapshot.previous_selection && (
              <p className="text-xs text-gray-500">Rollback restores Profile packages from before the last release.</p>
            )}
            <div className="flex justify-end gap-3 pt-2">
              <button type="button" onClick={() => void submit(true)} disabled={saving || !snapshot.previous_selection}
                className="rounded-lg border px-4 py-2 text-sm text-gray-700 disabled:opacity-50">Rollback</button>
              <button type="submit" disabled={saving}
                className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white disabled:opacity-50">
                {saving ? "Verifying…" : "Update packages"}
              </button>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}
