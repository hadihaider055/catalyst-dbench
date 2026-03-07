import { useAppStore } from "@/stores/useAppStore";
import { cn, dbIcon } from "@/lib/utils";
import type { QueryTab } from "@/lib/types";

export default function TabButton({ tab }: { tab: QueryTab }) {
  const { activeTabId, setActiveTab, closeTab } = useAppStore();
  const isActive = tab.id === activeTabId;

  return (
    <div
      className={cn(
        "flex items-center gap-1.5 px-2.5 h-7 text-xs rounded-t border-t cursor-pointer select-none group whitespace-nowrap max-w-[180px]",
        isActive
          ? "bg-surface text-text-primary border-accent border-t-2"
          : "bg-surface-raised text-text-secondary border-surface-border hover:bg-surface-overlay",
      )}
      onClick={() => setActiveTab(tab.id)}
      title={`${tab.title} — ${tab.connection_name}`}
    >
      <span className="flex-shrink-0 text-[10px] opacity-60">{dbIcon(tab.db_type)}</span>
      <span className="truncate flex-1 min-w-0">{tab.title}</span>
      {tab.running && <span className="text-accent animate-pulse flex-shrink-0">●</span>}
      <span
        className="opacity-0 group-hover:opacity-100 hover:text-red-400 flex-shrink-0 transition-opacity leading-none"
        onClick={(e) => { e.stopPropagation(); closeTab(tab.id); }}
        title="Close tab"
      >
        ×
      </span>
    </div>
  );
}
