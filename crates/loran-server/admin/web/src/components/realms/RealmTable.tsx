import type { RealmInfo } from "../../lib/api";
import { cn } from "../../lib/utils";
import { Archive, Pause, Play, UserPlus, Users } from "lucide-react";

interface RealmTableProps {
  realms: RealmInfo[];
  writesEnabled: boolean;
  onToggleEnabled: (realm: RealmInfo) => void;
  onIssueOwnerInvite: (realm: string) => void;
  onRetire: (realm: string) => void;
  onShowMembers: (realm: string) => void;
}

function statusLabel(realm: RealmInfo): { text: string; className: string } {
  if (realm.status === "retired") {
    return { text: "Retired", className: "bg-gray-100 text-gray-500 border-gray-200" };
  }
  if (!realm.enabled) {
    return { text: "Suspended", className: "bg-amber-50 text-amber-700 border-amber-200" };
  }
  if (realm.status === "pending") {
    return { text: "Pending", className: "bg-blue-50 text-blue-700 border-blue-200" };
  }
  return { text: "Active", className: "bg-green-50 text-green-700 border-green-200" };
}

const actionClass =
  "rounded-lg p-1.5 text-gray-400 transition-colors disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-gray-400";

export function RealmTable({
  realms,
  writesEnabled,
  onToggleEnabled,
  onIssueOwnerInvite,
  onRetire,
  onShowMembers,
}: RealmTableProps) {
  if (realms.length === 0) {
    return (
      <div className="rounded-xl border border-gray-200 bg-white p-8 text-center text-sm text-gray-500">
        {writesEnabled
          ? "No realms yet. Create one to get started."
          : "No realms have been synced from superv yet."}
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-gray-200 bg-white overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-gray-200 bg-gray-50">
            <th className="px-4 py-3 text-left font-medium text-gray-500">Realm</th>
            <th className="px-4 py-3 text-left font-medium text-gray-500">Status</th>
            <th className="px-4 py-3 text-left font-medium text-gray-500">Created</th>
            <th className="px-4 py-3 text-right font-medium text-gray-500">Actions</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-100">
          {realms.map((realm) => {
            const retired = realm.status === "retired";
            const status = statusLabel(realm);
            const disabledTitle = retired ? "Retired" : "Managed by superv";
            const canWrite = writesEnabled && !retired;

            return (
              <tr key={realm.realm} className={cn("transition-colors", retired ? "text-gray-400" : "hover:bg-gray-50")}>
                <td className={cn("px-4 py-3 font-mono", retired ? "text-gray-400" : "text-gray-700")}>{realm.realm}</td>
                <td className="px-4 py-3">
                  <span className={cn("rounded border px-2 py-0.5 text-xs font-medium", status.className)}>
                    {status.text}
                  </span>
                </td>
                <td className="px-4 py-3 text-xs text-gray-500">
                  {new Date(realm.created_at * 1000).toLocaleDateString()}
                </td>
                <td className="px-4 py-3 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => onShowMembers(realm.realm)}
                      className={cn(actionClass, "hover:bg-gray-100 hover:text-gray-600")}
                      title="Members"
                    >
                      <Users className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => onToggleEnabled(realm)}
                      disabled={!canWrite}
                      className={cn(actionClass, "hover:bg-gray-100 hover:text-gray-600")}
                      title={canWrite ? (realm.enabled ? "Suspend" : "Resume") : disabledTitle}
                    >
                      {realm.enabled ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                    </button>
                    <button
                      onClick={() => onIssueOwnerInvite(realm.realm)}
                      disabled={!canWrite || realm.status !== "active"}
                      className={cn(actionClass, "hover:bg-blue-50 hover:text-blue-600")}
                      title={canWrite ? "Issue owner invite" : disabledTitle}
                    >
                      <UserPlus className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => onRetire(realm.realm)}
                      disabled={!canWrite}
                      className={cn(actionClass, "hover:bg-red-50 hover:text-red-600")}
                      title={canWrite ? "Retire" : disabledTitle}
                    >
                      <Archive className="h-4 w-4" />
                    </button>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
