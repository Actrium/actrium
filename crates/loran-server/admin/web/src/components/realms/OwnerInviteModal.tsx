import { useEffect } from "react";
import { X, Copy, Download, AlertTriangle, UserPlus } from "lucide-react";

interface OwnerInviteModalProps {
  realm: string;
  /** The owner invite text, `ri_…`. */
  invite: string;
  /** RFC 3339 expiry. */
  expiresAt?: string | null;
  onClose: () => void;
}

/**
 * Shows an owner invite once. The operator hands it to the Realm's owner,
 * who redeems it with a new client; earlier unused owner invites stop
 * working.
 */
export function OwnerInviteModal({ realm, invite, expiresAt, onClose }: OwnerInviteModalProps) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  function handleDownload() {
    const blob = new Blob([invite], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `realm-${realm}-owner-invite.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white p-6 shadow-xl">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-lg font-semibold text-gray-900 flex items-center gap-2">
            <UserPlus className="h-5 w-5 text-amber-500" />
            Owner Invite
          </h2>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <div className="space-y-6">
          <div className="rounded-lg bg-amber-50 border border-amber-200 p-4">
            <div className="flex items-start gap-3">
              <AlertTriangle className="h-5 w-5 text-amber-600 shrink-0 mt-0.5" />
              <div className="space-y-1">
                <p className="text-sm font-medium text-amber-900">Hand this invite to the owner</p>
                <p className="text-xs text-amber-700">
                  It is shown only once and can be redeemed once. Whoever redeems it becomes the
                  Realm&apos;s owner; an existing owner&apos;s clients are revoked. Earlier unused
                  owner invites no longer work.
                </p>
              </div>
            </div>
          </div>

          <div className="space-y-3">
            <div>
              <label className="text-xs font-medium text-gray-500 uppercase">Realm</label>
              <p className="text-sm font-mono text-gray-900">{realm}</p>
            </div>
            <div>
              <label className="text-xs font-medium text-gray-500 uppercase">Owner Invite</label>
              <code className="mt-1 block rounded bg-gray-100 px-3 py-2 text-sm font-mono text-gray-800 border border-gray-200 break-all">
                {invite}
              </code>
              {expiresAt && (
                <p className="mt-1 text-xs text-gray-500">
                  Expires {new Date(expiresAt).toLocaleString()}.
                </p>
              )}
              <div className="flex gap-2 mt-2">
                <button
                  onClick={() => navigator.clipboard.writeText(invite)}
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

          <div className="pt-4 border-t border-gray-100 flex justify-end">
            <button
              onClick={onClose}
              className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
