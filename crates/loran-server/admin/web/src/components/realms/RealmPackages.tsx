import { useEffect, useRef, useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { api, type RealmPackageSelection, type RealmPackageState } from "../../lib/api";

const input = "w-full rounded-lg border border-gray-300 px-3 py-2 text-sm font-mono";

interface PackageDraft {
  primaryPackage: string;
  frontPackage: string;
  packagesJson: string;
}

function parsePackages(text: string): RealmPackageSelection["packages"] {
  let packages: unknown;
  try {
    packages = JSON.parse(text);
  } catch {
    throw new Error("Enter valid JSON for the package map.");
  }
  if (packages === null || typeof packages !== "object" || Array.isArray(packages)) {
    throw new Error("Packages JSON must be an object keyed by package reference.");
  }
  return packages as RealmPackageSelection["packages"];
}

export function RealmPackages({ realm, onClose }: { realm: string; onClose: () => void }) {
  const [snapshot, setSnapshot] = useState<RealmPackageState>();
  const [draft, setDraft] = useState<PackageDraft>();
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const [saving, setSaving] = useState(false);
  const pending = useRef<{ key: string; id: string } | undefined>(undefined);

  function accept(state: RealmPackageState) {
    setSnapshot(state);
    setDraft({
      primaryPackage: state.selection.primary_package,
      frontPackage: state.selection.front_package,
      packagesJson: JSON.stringify(state.selection.packages, null, 2),
    });
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
    if (!snapshot || !draft || saving) return;
    setError("");
    setResult("");
    try {
      const target = rollback ? snapshot.previous_selection : {
        primary_package: draft.primaryPackage.trim(),
        front_package: draft.frontPackage.trim(),
        packages: parsePackages(draft.packagesJson),
      };
      if (!target) return;
      const key = JSON.stringify([target, snapshot.sequence, rollback]);
      if (pending.current?.key !== key) pending.current = { key, id: crypto.randomUUID().replace(/-/g, "") };
      setSaving(true);
      const response = await api.updateRealmPackages(realm, {
        selection: target,
        operation: { operation_id: pending.current.id, expected_sequence: snapshot.sequence, rollback },
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
      <section className="mx-4 max-h-[92vh] w-full max-w-3xl overflow-y-auto rounded-xl bg-white p-6 shadow-xl">
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
        {draft && snapshot && (
          <form onSubmit={handleSubmit} className="space-y-4">
            <fieldset disabled={saving} className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="grid gap-1 text-sm font-medium text-gray-700">
                  Primary Service reference
                  <input required spellCheck={false} value={draft.primaryPackage} className={input}
                    onChange={(event) => setDraft({ ...draft, primaryPackage: event.target.value })} />
                </label>
                <label className="grid gap-1 text-sm font-medium text-gray-700">
                  Front reference
                  <input required spellCheck={false} value={draft.frontPackage} className={input}
                    onChange={(event) => setDraft({ ...draft, frontPackage: event.target.value })} />
                </label>
              </div>
              <div className="grid gap-1">
                <label htmlFor="realm-packages-json" className="text-sm font-medium text-gray-700">Packages JSON</label>
                <textarea id="realm-packages-json" required spellCheck={false} wrap="off" rows={16}
                  aria-describedby="realm-packages-json-hint" value={draft.packagesJson} className={`${input} resize-y`}
                  onChange={(event) => setDraft({ ...draft, packagesJson: event.target.value })} />
                <p id="realm-packages-json-hint" className="text-xs text-gray-500">
                  Each key is a package reference. Use <code>dependencies</code> to map a package&apos;s aliases to references in this JSON.
                </p>
              </div>
            </fieldset>
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
