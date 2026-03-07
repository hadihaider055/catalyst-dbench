import { Clock, Trash } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { cn, formatDuration } from "@/lib/utils";
import type { HistoryEntry } from "@/lib/types";

export default function HistoryPanel() {
  const { history, clearHistory, openTab, activeConnections, tabs, setActiveTab, updateTab } = useAppStore();

  const rerunQuery = (entry: HistoryEntry) => {
    const existingTab = tabs.find((t) => t.connection_id === entry.conn_id);
    if (existingTab) {
      updateTab(existingTab.id, { sql: entry.sql });
      setActiveTab(existingTab.id);
    } else {
      const conn = activeConnections.find((c) => c.id === entry.conn_id);
      if (!conn) return;
      const tabId = openTab(entry.conn_id, entry.conn_name, conn.db_type);
      useAppStore.setState((s) => ({
        tabs: s.tabs.map((t) => t.id === tabId ? { ...t, sql: entry.sql } : t),
      }));
    }
  };

  return (
    <>
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-surface-border flex-shrink-0">
        <span className="text-2xs text-text-muted">{history.length} queries</span>
        {history.length > 0 && (
          <button className="text-text-muted hover:text-red-400 transition-colors p-0.5" title="Clear history" onClick={clearHistory}>
            <Trash size={12} />
          </button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto">
        {history.length === 0 ? (
          <div className="flex flex-col items-center gap-2 mt-8 px-4 text-center">
            <Clock size={24} className="text-text-muted" />
            <p className="text-xs text-text-muted">Run queries to see history</p>
          </div>
        ) : (
          history.map((entry) => (
            <HistoryRow key={entry.id} entry={entry} onRerun={() => rerunQuery(entry)} />
          ))
        )}
      </div>
    </>
  );
}

function HistoryRow({ entry, onRerun }: { entry: HistoryEntry; onRerun: () => void }) {
  const ts = new Date(entry.ts);
  const timeStr = ts.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  return (
    <div
      className="group px-3 py-2 border-b border-surface-border/50 hover:bg-surface-overlay cursor-pointer"
      onClick={onRerun}
      title="Click to load in editor"
    >
      <div className="flex items-center gap-1.5 mb-0.5">
        <span className={cn("w-1.5 h-1.5 rounded-full flex-shrink-0", entry.error ? "bg-red-400" : "bg-green-400")} />
        <span className="text-2xs text-text-muted truncate flex-1">{entry.conn_name}</span>
        <span className="text-2xs text-text-muted flex-shrink-0">{timeStr}</span>
      </div>
      <pre className="text-2xs text-text-secondary font-mono truncate leading-relaxed">
        {entry.sql.replace(/\s+/g, " ").slice(0, 80)}
      </pre>
      <div className="flex items-center gap-2 mt-0.5 text-2xs text-text-muted">
        <span>{formatDuration(entry.duration_ms)}</span>
        {!entry.error && <span>{entry.row_count} rows</span>}
        {entry.error && <span className="text-red-400 truncate">{entry.error.slice(0, 40)}</span>}
      </div>
    </div>
  );
}
