import { useState } from "react";
import { type AppProfile, type MemberAction, type PackageSelection } from "../../lib/api";
import { PackageCheck } from "./PackageCheck";

export function AppProfileEditor({
  profile,
  disabled,
  packageVerifierAvailable,
  onSave,
}: {
  profile: AppProfile;
  disabled: boolean;
  packageVerifierAvailable: boolean;
  onSave: (change: MemberAction) => Promise<void>;
}) {
  const [name, setName] = useState(profile.realm.display_name || "");
  const original = profile.packages
    ? JSON.stringify(
        {
          primary_package: profile.primary_package,
          front_package: profile.front_package,
          packages: profile.packages,
        },
        null,
        2,
      )
    : "";
  const [packages, setPackages] = useState(original);
  const [error, setError] = useState("");
  const changedName = name.trim() !== profile.realm.display_name;
  const changedPackages = packages.trim() !== original;
  return (
    <details>
      <summary className="cursor-pointer text-sm font-medium">Edit app profile</summary>
      <form
        className="mt-3 space-y-3"
        onSubmit={async (e) => {
          e.preventDefault();
          setError("");
          try {
            const selection: PackageSelection | undefined = changedPackages ? JSON.parse(packages) : undefined;
            await onSave({
              action: "update_app",
              ...(changedName && { display_name: name.trim() }),
              ...(selection && { selection }),
            });
          } catch (err) {
            setError(err instanceof Error ? err.message : "Profile update failed");
          }
        }}
      >
        <label className="grid gap-1 text-sm">
          App display name
          <input required className="rounded border p-2" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="grid gap-1 text-sm">
          Complete package selection
          <textarea
            aria-label="Complete package selection"
            rows={5}
            className="rounded border p-2 font-mono text-xs"
            value={packages}
            onChange={(e) => setPackages(e.target.value)}
          />
        </label>
        <PackageCheck selection={packages} available={packageVerifierAvailable} disabled={disabled} />
        <button
          className="rounded border px-3 py-2 text-sm disabled:opacity-50"
          disabled={disabled || (!changedName && !changedPackages)}
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
