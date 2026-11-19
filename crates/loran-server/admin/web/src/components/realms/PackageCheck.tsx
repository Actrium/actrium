import { useState } from "react";
import { api, type PackageSelection } from "../../lib/api";

export function PackageCheck({ selection, available, disabled = false }: {
  selection: string;
  available: boolean;
  disabled?: boolean;
}) {
  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<{ input: string; message: string; ok: boolean } | null>(null);
  async function check() {
    setPending(true);
    setResult(null);
    try {
      const parsed: PackageSelection = JSON.parse(selection);
      const response = await api.checkPackages(parsed);
      setResult({
        input: selection,
        message:
          response.count +
          " package archives verified, including Publisher signatures and Service / Front compatibility.",
        ok: true,
      });
    } catch (err) {
      setResult({
        input: selection,
        message: err instanceof Error ? err.message : "Package check failed",
        ok: false,
      });
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="space-y-1">
      <button
        type="button"
        className="rounded border px-3 py-2 text-sm disabled:opacity-50"
        disabled={!available || disabled || pending || !selection}
        onClick={() => void check()}
      >
        {pending ? "Checking archives…" : "Verify package archives"}
      </button>
      {!available && (
        <p className="text-xs text-gray-500">
          Package verification is unavailable on this node. Install <code>actr</code> with{" "}
          <code>realm check-selection</code> on the AFP server, or set <code>LORAN_ADMIN_ACTR_BIN</code>{" "}
          and restart Loran. You can still save the package selection.
        </p>
      )}
      {available && result?.input === selection && (
        <p role="status" className={result.ok ? "text-sm text-green-700" : "text-sm text-red-700"}>
          {result.message}
        </p>
      )}
    </div>
  );
}
