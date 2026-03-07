import { useState } from "react";
import { PlusCircle, Database } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import type { SavedConnection } from "@/lib/types";
import ConnectionItem from "./ConnectionItem";

interface Props {
  onAddDialog: () => void;
  onConnect: (conn: SavedConnection) => void;
  onEdit: (conn: SavedConnection) => void;
}

export default function ConnectionsPanel({ onAddDialog, onConnect, onEdit }: Props) {
  const {
    savedConnections, activeConnections, openTab,
    selectedConnectionId, setSelectedConnection,
  } = useAppStore();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  const toggle = (id: string) => setExpanded((e) => ({ ...e, [id]: !e[id] }));

  const findActive = (conn: SavedConnection) =>
    activeConnections.find((c) => c.id === conn.id) ??
    activeConnections.find(
      (c) => c.host === conn.host && c.database === conn.database && c.username === conn.username
    );

  const handleOpenTab = (conn: SavedConnection) => {
    const active = findActive(conn);
    if (active) openTab(active.id, conn.name, conn.db_type);
  };

  return (
    <>
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-surface-border flex-shrink-0">
        <span className="text-2xs text-text-muted">{savedConnections.length} saved</span>
        <button
          className="p-0.5 hover:text-accent text-text-muted transition-colors"
          onClick={onAddDialog}
          title="New connection"
        >
          <PlusCircle size={14} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto py-1">
        {savedConnections.length === 0 ? (
          <div className="flex flex-col items-center gap-3 mt-8 px-4 text-center">
            <Database size={28} className="text-text-muted" />
            <div>
              <p className="text-xs text-text-secondary font-medium">No connections</p>
              <p className="text-xs text-text-muted mt-0.5">Click + to add a database</p>
            </div>
            <button
              className="flex items-center gap-1.5 px-3 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-xs transition-colors"
              onClick={onAddDialog}
            >
              <PlusCircle size={12} /> Connect
            </button>
          </div>
        ) : (
          savedConnections.map((conn) => {
            const active = findActive(conn);
            return (
              <ConnectionItem
                key={conn.id}
                conn={conn}
                activeId={active?.id}
                selected={selectedConnectionId === conn.id}
                expanded={!!expanded[conn.id]}
                onExpand={() => toggle(conn.id)}
                onSelect={() => setSelectedConnection(conn.id)}
                onOpen={() => handleOpenTab(conn)}
                onConnect={() => onConnect(conn)}
                onEdit={() => onEdit(conn)}
              />
            );
          })
        )}
      </div>
    </>
  );
}
