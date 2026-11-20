import type { MouseEventHandler, ReactNode } from "react";
import { ChevronRight } from "lucide-react";

export const managementCard = "rounded-xl border border-gray-200 bg-white";
export const managementCell = "px-4 py-3 align-middle";
export const managementRow = "border-t border-gray-100";

const action = "inline-flex min-h-9 items-center justify-center gap-1 whitespace-nowrap rounded-lg border px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-50";
export const managementButton = {
  primary: action + " border-transparent bg-blue-600 text-white hover:bg-blue-700",
  secondary: action + " border-gray-300 bg-white text-gray-700 hover:bg-gray-50",
  danger: action + " border-red-200 bg-white text-red-700 hover:bg-red-50",
};

export function ManagementTable({ label, columns, children }: {
  label: string;
  columns: string[];
  children: ReactNode;
}) {
  return (
    <div className="overflow-x-auto [container-type:inline-size]" role="region" aria-label={label + " table"} tabIndex={0}>
      <table className="w-full text-left text-sm" aria-label={label}>
        <thead className="bg-gray-50 text-gray-600">
          <tr>
            {columns.map(column => (
              <th key={column} scope="col" className="whitespace-nowrap px-4 py-3 font-medium">{column}</th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}

const badgeTone = {
  success: "bg-green-100 text-green-800",
  warning: "bg-orange-100 text-orange-800",
  danger: "bg-red-100 text-red-800",
  neutral: "bg-gray-100 text-gray-600",
  info: "bg-blue-50 text-blue-700",
};
export const managementBadge = "inline-flex items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium";

export function ManagementStatus({ tone, children }: { tone: keyof typeof badgeTone; children: ReactNode }) {
  return <span className={managementBadge + " " + badgeTone[tone]}>{children}</span>;
}

export function ManagementDisclosure({ expanded, controls, onClick, children, className = "" }: {
  expanded: boolean;
  controls: string;
  onClick: MouseEventHandler<HTMLButtonElement>;
  children: ReactNode;
  className?: string;
}) {
  return (
    <button
      type="button"
      aria-expanded={expanded}
      aria-controls={controls}
      onClick={onClick}
      className={"flex min-h-9 items-center gap-2 rounded-md text-left text-sm font-medium text-gray-800 hover:text-blue-700 " + className}
    >
      <ChevronRight size={16} aria-hidden="true" className={"shrink-0 text-gray-500 transition-transform" + (expanded ? " rotate-90" : "")} />
      {children}
    </button>
  );
}
