import { useState } from "react";
import { type AppProfile, type MemberAction } from "../../lib/api";
import { managementButton } from "../ui/ManagementControls";

export function AppProfileEditor({
  profile,
  disabled,
  onSave,
}: {
  profile: AppProfile;
  disabled: boolean;
  onSave: (change: MemberAction) => Promise<void>;
}) {
  const [name, setName] = useState(profile.realm.display_name || "");
  const [error, setError] = useState("");
  const changedName = name.trim() !== profile.realm.display_name;
  return (
    <details>
      <summary className="cursor-pointer text-sm font-medium">Edit app profile</summary>
      <form
        className="mt-3 space-y-3"
        onSubmit={async (e) => {
          e.preventDefault();
          setError("");
          try {
            await onSave({ action: "update_app", display_name: name.trim() });
          } catch (err) {
            setError(err instanceof Error ? err.message : "Profile update failed");
          }
        }}
      >
        <label className="grid gap-1 text-sm">
          App display name
          <input required className="rounded border p-2" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <button
          className={managementButton.primary}
          disabled={disabled || !changedName}
        >
          Save signed profile
        </button>
        {error && (
          <p role="alert" className="text-sm text-red-700">
            {error}
          </p>
        )}
      </form>
    </details>
  );
}
