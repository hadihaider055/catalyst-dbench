import { useState } from "react";
import { PlusCircle, Database, ChevronRight, ChevronDown, Plug, PlugZap, Trash2, RefreshCw, Lock } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { cn, dbIcon, dbColor } from "@/lib/utils";
import type { SavedConnection } from "@/lib/types";
import ConnectionDialog from "./ConnectionDialog";

export default function Sidebar() {
  const { savedConnections, activeConnections, openTab, removeActiveConnection, selectedConnectionId, setSelectedConnection } = useAppStore();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  const toggle = (id: string) => setExpanded((e) => ({ ...e, [id]: !e[id] }));
  const isActive = (id: string) => activeConnections.some((c) => c.id === id);

  return (
    <div className="h-full flex flex-col bg-surface-raised border-r border-surface-border overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-surface-border flex-shrink-0">
        <span className="text-2xs font-bold text-text-muted uppercase tracking-widest">Connections</span>
        <button
          className="p-0.5 hover:text-accent text-text-muted transition-colors"
          onClick={() => setShowAddDialog(true)}
          title="New connection"
        >
          <PlusCircle size={14} />
        </button>
      </div>

      {/* Connection list */}
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
              onClick={() => setShowAddDialog(true)}
            >
              <PlusCircle size={12} /> Connect
            </button>
          </div>
        ) : (
          savedConnections.map((conn) => (
            <ConnectionItem
              key={conn.id}
              conn={conn}
              active={isActive(conn.id)}
              selected={selectedConnectionId === conn.id}
              expanded={!!expanded[conn.id]}
              onExpand={() => toggle(conn.id)}
              onSelect={() => setSelectedConnection(conn.id)}
              onOpen={() => openTab(conn.id, conn.name, conn.db_type)}
            />
          ))
        )}
      </div>

      {showAddDialog && <ConnectionDialog onClose={() => setShowAddDialog(false)} />}
    </div>
  );
}

interface ConnectionItemProps {
  conn: SavedConnection;
  active: boolean;
  selected: boolean;
  expanded: boolean;
  onExpand: () => void;
  onSelect: () => void;
  onOpen: () => void;
}

function ConnectionItem({ conn, active, selected, expanded, onExpand, onSelect, onOpen }: ConnectionItemProps) {
  const { removeSavedConnection, removeActiveConnection } = useAppStore();

  const handleDisconnect = (e: React.MouseEvent) => {
    e.stopPropagation();
    removeActiveConnection(conn.id);
  };

  const handleRemove = (e: React.MouseEvent) => {
    e.stopPropagation();
    removeActiveConnection(conn.id);
    removeSavedConnection(conn.id);
  };

  return (
    <div className={cn("group", selected && "bg-accent-muted")}>
      <div
        className={cn(
          "flex items-center gap-1.5 px-2 py-1.5 cursor-pointer hover:bg-surface-overlay transition-colors",
          selected && "bg-accent-muted"
        )}
        onClick={() => { onSelect(); onExpand(); }}
        onDoubleClick={onOpen}
      >
        <button
          className="text-text-muted hover:text-text-primary flex-shrink-0"
          onClick={(e) => { e.stopPropagation(); onExpand(); }}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>

        {/* DB icon with color dot */}
        <span className="text-sm flex-shrink-0" style={{ filter: `drop-shadow(0 0 3px ${dbColor(conn.db_type)})` }}>
          {dbIcon(conn.db_type)}
        </span>

        <span className="flex-1 text-xs text-text-primary truncate font-medium">{conn.name}</span>

        {/* Status indicators */}
        {conn.tls_enabled && <Lock size={10} className="text-text-muted flex-shrink-0" title="TLS enabled" />}
        {conn.read_only && <span className="text-2xs text-yellow-500 flex-shrink-0">RO</span>}

        {/* Active indicator */}
        <div className={cn("w-1.5 h-1.5 rounded-full flex-shrink-0", active ? "bg-green-400" : "bg-text-muted")} />

        {/* Actions on hover */}
        <div className="hidden group-hover:flex items-center gap-1 flex-shrink-0">
          {active ? (
            <button className="text-text-muted hover:text-red-400 p-0.5" onClick={handleDisconnect} title="Disconnect">
              <PlugZap size={11} />
            </button>
          ) : (
            <button className="text-text-muted hover:text-green-400 p-0.5" onClick={(e) => { e.stopPropagation(); onOpen(); }} title="Connect">
              <Plug size={11} />
            </button>
          )}
          <button className="text-text-muted hover:text-red-500 p-0.5" onClick={handleRemove} title="Remove">
            <Trash2 size={11} />
          </button>
        </div>
      </div>

      {/* Sub-items (schema tree placeholder) */}
      {expanded && active && (
        <div className="pl-6 py-1 text-xs text-text-muted">
          <SchemaPlaceholder conn={conn} />
        </div>
      )}
    </div>
  );
}

function SchemaPlaceholder({ conn }: { conn: SavedConnection }) {
  return (
    <div className="flex items-center gap-1.5 px-2 py-1 text-text-muted">
      <RefreshCw size={10} className="animate-spin" />
      <span>Loading schema…</span>
    </div>
  );
}
