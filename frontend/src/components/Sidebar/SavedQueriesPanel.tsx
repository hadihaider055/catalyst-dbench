import { useState } from "react";
import { Search, Trash2, Play, Copy } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { executeQuery } from "@/lib/commands";
import { cn, dbIcon } from "@/lib/utils";
import type { SavedQuery } from "@/lib/types";

export default function SavedQueriesPanel() {
  const { savedQueries, deleteSavedQuery, activeConnections, openTab } = useAppStore();
  const [filter, setFilter] = useState("");

  const filtered = filter.trim()
    ? savedQueries.filter(
        (q) =>
          q.name.toLowerCase().includes(filter.toLowerCase()) ||
          q.sql.toLowerCase().includes(filter.toLowerCase()),
      )
    : savedQueries;

  const runQuery = async (q: SavedQuery) => {
    const conn = activeConnections.find((c) => !q.db_type || c.db_type === q.db_type);
    if (!conn) return;
    const tabId = openTab(conn.id, conn.host, conn.db_type);
    useAppStore.setState((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, sql: q.sql } : t)),
    }));
    useAppStore.getState().updateTab(tabId, { running: true });
    try {
      const result = await executeQuery({ connection_id: conn.id, sql: q.sql });
      useAppStore.getState().updateTab(tabId, { result, running: false });
    } catch (e) {
      useAppStore.getState().updateTab(tabId, { error: String(e), running: false });
    }
  };

  const openQueryInTab = (q: SavedQuery) => {
    const conn = activeConnections.find((c) => !q.db_type || c.db_type === q.db_type);
    if (!conn) return;
    const tabId = openTab(conn.id, conn.host, conn.db_type);
    useAppStore.setState((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, sql: q.sql, title: q.name } : t)),
    }));
  };

  return (
    <div className="flex flex-col h-full">
      <div className="px-2 py-2 border-b border-surface-border">
        <div className="relative">
          <Search
            size={10}
            className="absolute left-2 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none"
          />
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Search saved queries…"
            className="w-full bg-surface border border-surface-border rounded pl-6 pr-2 py-1 text-2xs text-text-primary placeholder-text-muted outline-none focus:border-accent transition-colors"
          />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto py-0.5">
        {filtered.length === 0 ? (
          <div className="px-3 py-4 text-2xs text-text-muted italic text-center">
            {savedQueries.length === 0
              ? "No saved queries yet — use the Save button in the editor"
              : "No matches"}
          </div>
        ) : (
          filtered.map((q) => <QueryRow key={q.id} query={q} onRun={runQuery} onOpen={openQueryInTab} onDelete={deleteSavedQuery} />)
        )}
      </div>
    </div>
  );
}

function QueryRow({
  query,
  onRun,
  onOpen,
  onDelete,
}: {
  query: SavedQuery;
  onRun: (q: SavedQuery) => void;
  onOpen: (q: SavedQuery) => void;
  onDelete: (id: string) => void;
}) {
  const [confirm, setConfirm] = useState(false);

  return (
    <div
      className={cn(
        "group flex items-center gap-1.5 px-2 py-1.5 hover:bg-surface-overlay cursor-pointer transition-colors",
      )}
      onClick={() => onOpen(query)}
    >
      {query.db_type && (
        <span className="text-xs flex-shrink-0">{dbIcon(query.db_type)}</span>
      )}
      <div className="flex-1 min-w-0">
        <div className="text-2xs font-medium text-text-primary truncate">{query.name}</div>
        <div className="text-2xs text-text-muted truncate">{query.sql}</div>
      </div>

      <div className="hidden group-hover:flex items-center gap-0.5 flex-shrink-0">
        <button
          className="p-0.5 text-text-muted hover:text-green-400 transition-colors"
          title="Run query"
          onClick={(e) => {
            e.stopPropagation();
            onRun(query);
          }}
        >
          <Play size={10} />
        </button>
        <button
          className="p-0.5 text-text-muted hover:text-accent transition-colors"
          title="Copy SQL"
          onClick={(e) => {
            e.stopPropagation();
            navigator.clipboard.writeText(query.sql);
          }}
        >
          <Copy size={10} />
        </button>
        {confirm ? (
          <>
            <button
              className="px-1 py-0.5 text-2xs text-red-400 hover:text-red-300"
              onClick={(e) => {
                e.stopPropagation();
                onDelete(query.id);
              }}
            >
              Delete
            </button>
            <button
              className="px-1 py-0.5 text-2xs text-text-muted"
              onClick={(e) => {
                e.stopPropagation();
                setConfirm(false);
              }}
            >
              Cancel
            </button>
          </>
        ) : (
          <button
            className="p-0.5 text-text-muted hover:text-red-400 transition-colors"
            title="Delete"
            onClick={(e) => {
              e.stopPropagation();
              setConfirm(true);
            }}
          >
            <Trash2 size={10} />
          </button>
        )}
      </div>
    </div>
  );
}
