import type { RealmInfo } from "../../lib/api";

export function appStatus(app: RealmInfo) {
  if (app.status === "retired") return "Retired";
  if (app.status === "pending") return "Pending";
  return app.enabled ? "Active" : "Suspended";
}

export function RealmTable({ realms, onOpen }: { realms: RealmInfo[]; onOpen: (id: string) => void }) {
  if (!realms.length)
    return <p className="rounded-xl border bg-white p-6 text-sm text-gray-500">No hosted apps yet.</p>;
  return (
    <div className="overflow-x-auto rounded-xl border border-gray-200 bg-white">
      <table className="w-full text-left text-sm">
        <thead className="bg-gray-50 text-gray-600">
          <tr>
            {["Application", "App ID", "Status", "Admission", "Members", "Created"].map((label) => (
              <th key={label} className="px-4 py-3 font-medium">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {realms.map((app) => (
            <tr key={app.realm} className="border-t border-gray-100">
              <td className="px-4 py-3">
                <button onClick={() => onOpen(app.realm)} className="font-medium text-blue-700 hover:underline">
                  {app.display_name || app.realm}
                </button>
              </td>
              <td className="px-4 py-3 font-mono text-xs">{app.realm}</td>
              <td className="px-4 py-3">{appStatus(app)}</td>
              <td className="px-4 py-3">{app.policy.membership === "open" ? "Open" : "Invitation required"}</td>
              <td className="px-4 py-3 tabular-nums">{app.active_members} / {app.effective_max_members}</td>
              <td className="px-4 py-3">{new Date(app.created_at * 1000).toLocaleDateString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
