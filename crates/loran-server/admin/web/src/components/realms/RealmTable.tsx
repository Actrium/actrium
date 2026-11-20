import type { RealmInfo } from "../../lib/api";
import { ManagementStatus, ManagementTable, managementCard, managementCell, managementRow } from "../ui/ManagementControls";

export function appStatus(app: RealmInfo) {
  if (app.status === "retired") return "Retired";
  if (app.status === "pending") return "Pending";
  return app.enabled ? "Active" : "Suspended";
}

export function RealmTable({ realms, onOpen }: { realms: RealmInfo[]; onOpen: (id: string) => void }) {
  if (!realms.length)
    return <p className={managementCard + " p-6 text-sm text-gray-500"}>No hosted apps yet.</p>;
  const tones = { Active: "success", Suspended: "warning", Pending: "info", Retired: "neutral" } as const;
  return (
    <div className={"overflow-hidden " + managementCard}>
      <ManagementTable label="Apps" columns={["Application", "App ID", "Status", "Admission", "Members", "Created"]}>
        {realms.map((app) => {
          const status = appStatus(app);
          return (
            <tr key={app.realm} className={managementRow}>
              <td className={managementCell}>
                <button type="button" onClick={() => onOpen(app.realm)} className="flex min-h-9 items-center whitespace-nowrap text-left font-medium text-blue-700 hover:underline">
                  {app.display_name || app.realm}
                </button>
              </td>
              <td className={managementCell + " font-mono text-xs"}>{app.realm}</td>
              <td className={managementCell}><ManagementStatus tone={tones[status]}>{status}</ManagementStatus></td>
              <td className={managementCell + " whitespace-nowrap"}>{app.policy.membership === "open" ? "Open" : "Invitation required"}</td>
              <td className={managementCell + " whitespace-nowrap tabular-nums"}>{app.active_members} / {app.effective_max_members}</td>
              <td className={managementCell}>{new Date(app.created_at * 1000).toLocaleDateString()}</td>
            </tr>
          );
        })}
      </ManagementTable>
    </div>
  );
}
