import { useState, useEffect, useId, type ReactNode } from "react";
import { ChevronUp, ChevronDown } from "lucide-react";

const STORAGE_PREFIX = "loran_panel_";

export function CollapsibleCard({
  storageKey,
  title,
  defaultExpanded = true,
  headerRight,
  children,
}: {
  /** Unique key for localStorage persistence */
  storageKey: string;
  title: string;
  /** Initial state when no localStorage value exists */
  defaultExpanded?: boolean;
  /** Optional element rendered between title and chevron */
  headerRight?: ReactNode;
  children: ReactNode;
}) {
  const fullKey = STORAGE_PREFIX + storageKey;
  const contentId = useId();

  const [expanded, setExpanded] = useState(() => {
    try {
      const saved = localStorage.getItem(fullKey);
      return saved === null ? defaultExpanded : saved === "1";
    } catch {
      return defaultExpanded;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(fullKey, expanded ? "1" : "0");
    } catch {
      // ignore
    }
  }, [expanded, fullKey]);

  const Icon = expanded ? ChevronUp : ChevronDown;

  return (
    <div className="w-full rounded-xl border border-gray-200 bg-white">
      <div className="flex items-center gap-2 px-4 lg:px-5">
        <h2 className="cursor-text py-3 text-sm font-semibold text-gray-700">{title}</h2>
        {headerRight && <div className="ml-auto flex items-center">{headerRight}</div>}
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className={`group flex min-w-6 cursor-pointer items-center justify-end self-stretch rounded text-gray-400 hover:text-gray-600 ${headerRight ? "" : "flex-1"}`}
          aria-label={`${expanded ? "Collapse" : "Expand"} ${title}`}
          aria-expanded={expanded}
          aria-controls={contentId}
        >
          <Icon size={16} aria-hidden="true" className="box-content rounded p-0.5 transition-colors group-hover:bg-gray-100" />
        </button>
      </div>
      <div id={contentId} hidden={!expanded} className="px-4 pb-4 lg:px-5 lg:pb-5">
        {expanded && children}
      </div>
    </div>
  );
}
