import { useState, type FormEvent } from "react";
import { api, type RealmInfo, type RealmPolicy } from "../../lib/api";

interface PolicyFieldsProps {
  policy: RealmPolicy;
  limit: number;
  onChange: (value: RealmPolicy) => void;
}

export function RealmAdmissionFields({ policy, limit, onChange }: PolicyFieldsProps) {
  const inputClass = "mt-1 w-full rounded-lg border border-gray-300 px-3 py-2 text-sm";
  return (
    <fieldset className="grid gap-3 text-sm text-gray-700">
      <label>Membership
        <select className={inputClass} value={policy.membership}
          onChange={(e) => onChange({ ...policy, membership: e.target.value as RealmPolicy["membership"] })}>
          <option value="invite-only">Invitation required</option>
          <option value="open">Open (join as Viewer)</option>
        </select>
      </label>
      {policy.membership === "open" && (
        <label>Invite-free joins per minute
          <input className={inputClass} type="number" min="1" max="4294967295" placeholder="10"
            value={policy.open_joins_per_minute ?? ""}
            onChange={(e) => onChange({ ...policy, open_joins_per_minute: e.target.value ? e.target.valueAsNumber : null })} />
        </label>
      )}
      <label>Member limit
        <input className={inputClass} type="number" min="1" max={limit} placeholder={String(limit)}
          value={policy.max_members ?? ""}
          onChange={(e) => onChange({ ...policy, max_members: e.target.value ? e.target.valueAsNumber : null })} />
      </label>
    </fieldset>
  );
}

export function EditRealmAdmission({ realm, limit, onClose, onSaved }: {
  realm: RealmInfo;
  limit: number;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [policy, setPolicy] = useState(realm.policy);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  async function save(e: FormEvent) {
    e.preventDefault();
    setSaving(true);
    try {
      await api.setRealmPolicy(realm.realm, policy);
      onSaved();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not save admission settings");
    } finally {
      setSaving(false);
    }
  }
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <form role="dialog" aria-modal="true" aria-label="Edit admission" onSubmit={save}
        className="w-full max-w-md space-y-4 rounded-xl bg-white p-6 shadow-xl">
        <h2 className="text-lg font-semibold text-gray-900">Admission for {realm.realm}</h2>
        <RealmAdmissionFields policy={policy} limit={limit} onChange={setPolicy} />
        {error && <p role="alert" className="text-sm text-red-600">{error}</p>}
        <div className="flex justify-end gap-3">
          <button type="button" onClick={onClose} className="rounded-lg border px-4 py-2">Cancel</button>
          <button disabled={saving} className="rounded-lg bg-blue-600 px-4 py-2 text-white disabled:opacity-50">Save</button>
        </div>
      </form>
    </div>
  );
}
