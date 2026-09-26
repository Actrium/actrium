import { useEffect, useState } from "react";
import { X, Users } from "lucide-react";
import { api, type RealmMembersResponse } from "../../lib/api";
import { cn } from "../../lib/utils";

interface RealmMembersModalProps {
  realm: string;
  onClose: () => void;
}

function formatTime(value: string | null): string {
  return value ? new Date(value).toLocaleString() : "Never";
}

/**
 * Read-only view of a Realm's members: role, status and their clients.
 * Members manage themselves with `actr realm`; nothing here changes them.
 */
export function RealmMembersModal({ realm, onClose }: RealmMembersModalProps) {
  const [data, setData] = useState<RealmMembersResponse | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    api
      .listRealmMembers(realm)
      .then((resp) => {
        if (!cancelled) setData(resp);
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : "Failed to load members");
      });
    return () => {
      cancelled = true;
    };
  }, [realm]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-3xl rounded-xl bg-white p-6 shadow-xl">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-gray-900 flex items-center gap-2">
            <Users className="h-5 w-5 text-blue-500" />
            Members of <span className="font-mono">{realm}</span>
          </h2>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600 transition-colors"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {error && (
          <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
            {error}
          </div>
        )}
        {!data && !error && <div className="text-sm text-gray-500">Loading...</div>}

        {data && (
          <div className="space-y-3">
            {data.members.length === 0 ? (
              <p className="text-sm text-gray-500">
                No members yet: the owner invite has not been redeemed.
              </p>
            ) : (
              <div className="max-h-[60vh] overflow-auto rounded-lg border border-gray-200">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-gray-200 bg-gray-50">
                      <th className="px-3 py-2 text-left font-medium text-gray-500">Member</th>
                      <th className="px-3 py-2 text-left font-medium text-gray-500">Role</th>
                      <th className="px-3 py-2 text-left font-medium text-gray-500">Status</th>
                      <th className="px-3 py-2 text-left font-medium text-gray-500">Clients</th>
                      <th className="px-3 py-2 text-left font-medium text-gray-500">Last used</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-100">
                    {data.members.map((member) => {
                      const active = member.clients.filter((c) => !c.revoked_at).length;
                      const lastUsed =
                        member.clients
                          .map((c) => c.last_used_at)
                          .filter((t): t is string => t !== null)
                          .sort()
                          .pop() ?? null;
                      const removed = member.status === "removed";
                      return (
                        <tr key={member.member_id} className={cn(removed && "text-gray-400")}>
                          <td className="px-3 py-2 font-mono text-xs">{member.member_id}</td>
                          <td className="px-3 py-2">{member.role}</td>
                          <td className="px-3 py-2">{member.status}</td>
                          <td
                            className="px-3 py-2"
                            title={member.clients.map((c) => c.label ?? c.key_id).join(", ")}
                          >
                            {active} active / {member.clients.length}
                          </td>
                          <td className="px-3 py-2 text-xs">{formatTime(lastUsed)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
            <p className="text-xs text-gray-500">
              {data.invites.length} open invite{data.invites.length === 1 ? "" : "s"}. Members
              manage invites, clients and roles themselves with <code>actr realm</code>.
            </p>
          </div>
        )}

        <div className="pt-4 mt-4 border-t border-gray-100 flex justify-end">
          <button
            onClick={onClose}
            className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
