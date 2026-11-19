import { useState, type FormEvent, type ReactNode, useEffect } from "react";
import { api, type AppPackage, type PackageSelection, type RealmMutationResponse, type RealmPolicy } from "../../lib/api";
import { X, Copy, Download, CheckCircle, AlertTriangle } from "lucide-react";

import { copyText } from "../../lib/utils";
import { PackageCheck } from "./PackageCheck";
import { RealmAdmissionFields } from "./RealmAdmission";

interface RealmFormProps {
  onSubmit: (data: Parameters<typeof api.createRealm>[0]) => Promise<RealmMutationResponse>;
  onClose: () => void;
  provider?: string | null;
  limit: number;
  packageVerifierAvailable: boolean;
  onRefresh?: () => void;
}

const emptyPackage = (): AppPackage => ({ href: "", sha256: "", type: "" });
const packageFields = ["href", "sha256", "type"] as const;
type PackageField = (typeof packageFields)[number];
const maxPackages = 32;
const packageReference = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;

const packagePlaceholders: Record<PackageField, string> = {
  href: "HTTPS download URL",
  sha256: "SHA-256 (64 lowercase hex digits)",
  type: "publisher.example/program@1.0.0",
};

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

const hasOwn = (object: object, key: string) => Object.prototype.hasOwnProperty.call(object, key);

type ParsedSelection =
  | { selection: PackageSelection; error: null }
  | { selection: null; error: string };

function parseSelection(text: string): ParsedSelection {
  const invalid = (error: string): ParsedSelection => ({ selection: null, error });
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return invalid("Package selection is not valid JSON.");
  }
  if (!isObject(value)) return invalid("Package selection must be an object.");
  const { primary_package, front_package, packages } = value;
  if (
    typeof primary_package !== "string" || typeof front_package !== "string" ||
    !isObject(packages) || primary_package === front_package ||
    !hasOwn(packages, primary_package) || !hasOwn(packages, front_package)
  ) {
    return invalid("primary_package and front_package must refer to two distinct package entries.");
  }
  const entries = Object.entries(packages);
  if (entries.length < 2 || entries.length > maxPackages) {
    return invalid("Package selection must contain 2–32 packages.");
  }
  for (const [ref, pkg] of entries) {
    if (!packageReference.test(ref)) return invalid(`Invalid package reference: ${ref}.`);
    if (!isObject(pkg) || packageFields.some((field) => typeof pkg[field] !== "string")) {
      return invalid(`Package "${ref}" needs string href, sha256 and type fields.`);
    }
    if (pkg.dependencies !== undefined) {
      if (!isObject(pkg.dependencies)) return invalid(`Dependencies of "${ref}" must be an object.`);
      for (const [alias, target] of Object.entries(pkg.dependencies)) {
        if (!alias || alias.trim() !== alias || typeof target !== "string" || !hasOwn(packages, target)) {
          return invalid(`Package "${ref}" has an invalid dependency binding for "${alias}".`);
        }
      }
      if (ref === front_package && pkg.dependencies.service !== undefined &&
          pkg.dependencies.service !== primary_package) {
        return invalid("The Front's service dependency must point to the primary Service.");
      }
    }
  }
  // Every editable field was checked above. Keep each whole entry, including
  // dependency bindings and extensions that have no field in the simple form.
  return { selection: { primary_package, front_package, packages: packages as Record<string, AppPackage> }, error: null };
}

function hasDependent(selection: PackageSelection, target: string): boolean {
  return Object.entries(selection.packages).some(([ref, pkg]) =>
    ref !== target && Object.values(pkg.dependencies ?? {}).includes(target),
  );
}

function PackageInputs({ pkg, label, prefix, required, onChange, children }: {
  pkg: AppPackage;
  label: string;
  prefix: string;
  required: boolean;
  onChange: (field: PackageField, value: string) => void;
  children?: ReactNode;
}) {
  return (
    <fieldset className="grid gap-1">
      <legend className="text-sm font-medium text-gray-700">{label}</legend>
      {packageFields.map((field) => (
        <input
          key={field}
          type={field === "href" ? "url" : "text"}
          aria-label={prefix + " " + field}
          required={required}
          value={pkg[field]}
          pattern={field === "sha256" ? "[0-9a-f]{64}" : undefined}
          placeholder={packagePlaceholders[field]}
          onChange={(event) => onChange(field, event.target.value.trim())}
          className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm font-mono"
        />
      ))}
      {children}
    </fieldset>
  );
}

/**
 * Creates a Realm. A Realm name is fixed at creation and never reused. A
 * Realm under the Provider domain gets a Loran-signed Realm Profile, which
 * names its Service and Front packages.
 */
export function RealmForm({ onSubmit, onClose, onRefresh, provider, limit, packageVerifierAvailable }: RealmFormProps) {
  const [policy, setPolicy] = useState<RealmPolicy>({ membership: "invite-only" });
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [selection, setSelection] = useState<PackageSelection>(() => ({
    primary_package: "service",
    front_package: "front",
    packages: { service: emptyPackage(), front: emptyPackage() },
  }));
  const [advanced, setAdvanced] = useState(false);
  const [selectionText, setSelectionText] = useState("");
  const jsonSelection = advanced ? parseSelection(selectionText) : null;
  const activeSelection = jsonSelection ? jsonSelection.selection : selection;
  const extraServices = Object.keys(selection.packages).filter(
    (ref) => ref !== selection.primary_package && ref !== selection.front_package,
  );
  const selectingPackages = Boolean(provider) || extraServices.length > 0 ||
    Object.values(selection.packages).some((pkg) => Object.values(pkg).some(Boolean));
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  // Wizard state
  const [step, setStep] = useState<"input" | "success">("input");
  const [ownerInvite, setOwnerInvite] = useState<string | null>(null);
  const [inviteExpiresAt, setInviteExpiresAt] = useState<string | null>(null);
  const [createdRealm, setCreatedRealm] = useState<string | null>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  function toggleJson(checked: boolean) {
    if (checked) {
      setSelectionText(JSON.stringify(selection, null, 2));
      setAdvanced(true);
    } else if (jsonSelection?.selection) {
      setSelection(jsonSelection.selection);
      setAdvanced(false);
    }
  }

  function updatePackage(ref: string, field: PackageField, value: string) {
    setSelection((current) => ({
      ...current,
      packages: { ...current.packages, [ref]: { ...current.packages[ref], [field]: value } },
    }));
  }

  function addService() {
    setSelection((current) => {
      if (Object.keys(current.packages).length >= maxPackages) return current;
      let index = 2;
      while (hasOwn(current.packages, `service-${index}`)) index++;
      return { ...current, packages: { ...current.packages, [`service-${index}`]: emptyPackage() } };
    });
  }

  function removeService(ref: string) {
    setSelection((current) => {
      if (ref === current.primary_package || ref === current.front_package || hasDependent(current, ref)) {
        return current;
      }
      const packages = { ...current.packages };
      delete packages[ref];
      return { ...current, packages };
    });
  }

  // The Realm is created suspended so the owner invite can be saved before
  // it serves Actors; the second step resumes it.
  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      if (provider && name.split("@")[1] !== provider)
        throw Error(
          "Create an app under " + provider + " here. For another domain use External domain · signed requests.",
        );
      if (!activeSelection) throw Error(jsonSelection?.error ?? "Invalid package selection.");
      const resp = await onSubmit({
        realm: name,
        policy,
        enabled: false,
        ...(displayName ? { display_name: displayName } : {}),
        ...((advanced || selectingPackages) ? activeSelection : {}),
      });
      if (!resp.success) throw Error(resp.error_message || "App creation failed");
      setCreatedRealm(resp.realm?.realm ?? name);
      if (resp.owner_invite) {
        setOwnerInvite(resp.owner_invite);
        setInviteExpiresAt(resp.owner_invite_expires_at ?? null);
      }
      setStep("success");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Operation failed");
    } finally {
      setLoading(false);
    }
  }

  async function handleActivate() {
    if (!createdRealm) return;
    setLoading(true);
    try {
      await api.updateRealm(createdRealm, { enabled: true });
      if (onRefresh) onRefresh();
      onClose();
    } catch {
      setError("Failed to resume app");
    } finally {
      setLoading(false);
    }
  }

  function handleDownload() {
    if (!ownerInvite || !createdRealm) return;
    const blob = new Blob([ownerInvite], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `realm-${createdRealm}-owner-invite.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function handleCopy() {
    if (ownerInvite) {
      void copyText(ownerInvite).catch(() => setError("Select the invite code and copy it manually."));
    }
  }

  if (step === "success" && !ownerInvite) {
    return (
      <div role="dialog" className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
        <div className="max-w-md space-y-4 rounded-xl bg-white p-6">
          <h2 className="font-semibold">App created. Owner invite unavailable.</h2>
          <p className="text-sm">
            {createdRealm} is suspended. Open the app to review its status and use Owner recovery if needed.
          </p>
          <button type="button" onClick={onClose} className="rounded border px-3 py-2">
            Close
          </button>
        </div>
      </div>
    );
  }

  if (step === "success" && ownerInvite && createdRealm) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
        <div className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-xl bg-white p-6 shadow-xl">
          <div className="flex items-center justify-between mb-6">
            <h2 className="text-lg font-semibold text-gray-900 flex items-center gap-2">
              <CheckCircle className="h-5 w-5 text-green-500" />
              App created
            </h2>
          </div>

          <div className="space-y-6">
            <div className="rounded-lg bg-amber-50 border border-amber-200 p-4">
              <div className="flex items-start gap-3">
                <AlertTriangle className="h-5 w-5 text-amber-600 shrink-0 mt-0.5" />
                <div className="space-y-1">
                  <p className="text-sm font-medium text-amber-900">Save the owner invite now</p>
                  <p className="text-xs text-amber-700">
                    It is shown only once. Hand it to the app&apos;s Owner, who redeems it with a new client
                    to claim ownership.
                  </p>
                </div>
              </div>
            </div>

            <div className="space-y-3">
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">App ID</label>
                <p className="text-sm font-mono text-gray-900">{createdRealm}</p>
              </div>
              <div>
                <label className="text-xs font-medium text-gray-500 uppercase">Owner Invite</label>
                <div className="mt-1 flex items-center gap-2">
                  <code className="flex-1 rounded bg-gray-100 px-3 py-2 text-sm font-mono text-gray-800 border border-gray-200 break-all">
                    {ownerInvite}
                  </code>
                </div>
                {inviteExpiresAt && (
                  <p className="mt-1 text-xs text-gray-500">
                    Expires {new Date(inviteExpiresAt).toLocaleString()}.
                  </p>
                )}
                <div className="flex gap-2 mt-2">
                  <button
                    onClick={handleCopy}
                    className="flex items-center gap-1.5 rounded border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 transition-colors"
                  >
                    <Copy className="h-3.5 w-3.5" /> Copy
                  </button>
                  <button
                    onClick={handleDownload}
                    className="flex items-center gap-1.5 rounded border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 transition-colors"
                  >
                    <Download className="h-3.5 w-3.5" /> Download
                  </button>
                </div>
              </div>
            </div>

            <div className="pt-4 border-t border-gray-100 flex flex-col gap-3">
              <button
                onClick={handleActivate}
                disabled={loading}
                className="w-full rounded-lg bg-blue-600 px-4 py-2.5 text-sm font-medium text-white hover:bg-blue-700 transition-colors disabled:opacity-50"
              >
                {loading ? "Resuming..." : "I have saved the invite, resume app"}
              </button>
              <button
                onClick={onClose}
                className="text-xs text-gray-500 hover:text-gray-700 hover:underline text-center"
              >
                Keep suspended for now
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-xl bg-white p-6 shadow-xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-gray-900">Create app</h2>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">App ID</label>
            <input
              type="text"
              aria-label="App ID"
              value={name}
              onChange={(e) => setName(e.target.value.trim())}
              placeholder="realm-id@example.com"
              title="<realm_id>@<domain>"
              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 disabled:bg-gray-50 disabled:text-gray-500 font-mono"
              required
              autoFocus
            />
            <p className="mt-1 text-xs text-gray-500">
              Use {provider ? "an ID ending in @" + provider : "the full Realm name"}. The App ID stays fixed,
              including after retirement.
            </p>
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Display Name <span className="font-normal text-gray-400">(optional)</span>
            </label>
            <input
              type="text"
              aria-label="Display name"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>

          <div className="space-y-2 border-t border-gray-100 pt-4">
            <h3 className="text-sm font-semibold text-gray-900">Admission</h3>
            <RealmAdmissionFields policy={policy} limit={limit} onChange={setPolicy} />
          </div>

          <div className="grid gap-3">
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={advanced}
                disabled={loading}
                onChange={(e) => toggleJson(e.target.checked)}
              />
              Edit complete package selection
            </label>
            {advanced ? (
              <>
                <label className="grid gap-1 text-sm">
                  Package selection JSON (2–32 packages)
                  <textarea
                    required
                    rows={9}
                    value={selectionText}
                    aria-invalid={Boolean(jsonSelection?.error)}
                    aria-describedby={jsonSelection?.error ? "package-selection-error" : undefined}
                    onChange={(e) => setSelectionText(e.target.value)}
                    className="rounded-lg border p-2 font-mono text-xs"
                  />
                </label>
                {jsonSelection?.error && (
                  <p id="package-selection-error" role="alert" className="text-sm text-red-600">
                    {jsonSelection.error}
                  </p>
                )}
              </>
            ) : (
              <>
                <PackageInputs
                  pkg={selection.packages[selection.primary_package]}
                  label="Main Service package"
                  prefix="service"
                  required={selectingPackages}
                  onChange={(field, value) => updatePackage(selection.primary_package, field, value)}
                />
                {extraServices.map((ref) => {
                  const referenced = hasDependent(selection, ref);
                  return (
                    <PackageInputs
                      key={ref}
                      pkg={selection.packages[ref]}
                      label={`Service package · ${ref}`}
                      prefix={`service package ${ref}`}
                      required={selectingPackages}
                      onChange={(field, value) => updatePackage(ref, field, value)}
                    >
                      <button
                        type="button"
                        aria-label={`Remove Service package ${ref}`}
                        disabled={loading || referenced}
                        className="justify-self-start rounded border px-2 py-1 text-xs disabled:opacity-50"
                        onClick={() => removeService(ref)}
                      >
                        Remove
                      </button>
                      {referenced && (
                        <p className="text-xs text-gray-500">
                          Remove references to this package in JSON before deleting it.
                        </p>
                      )}
                    </PackageInputs>
                  );
                })}
                <button
                  type="button"
                  disabled={loading || Object.keys(selection.packages).length >= maxPackages}
                  className="justify-self-start rounded-lg border px-3 py-2 text-sm disabled:opacity-50"
                  onClick={addService}
                >
                  Add Service package
                </button>
                <PackageInputs
                  pkg={selection.packages[selection.front_package]}
                  label="Front package"
                  prefix="front"
                  required={selectingPackages}
                  onChange={(field, value) => updatePackage(selection.front_package, field, value)}
                />
              </>
            )}
            <p className="text-xs text-gray-500">
              A signed App needs one main Service and one Front.
            </p>
          </div>

          {(advanced || selectingPackages) && (
            <PackageCheck
              available={packageVerifierAvailable}
              selection={activeSelection ? JSON.stringify(activeSelection) : ""}
              disabled={loading}
            />
          )}
          {error && <p className="text-sm text-red-600">{error}</p>}

          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading || !activeSelection}
              className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50 transition-colors"
            >
              {loading ? "Saving..." : "Next"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
