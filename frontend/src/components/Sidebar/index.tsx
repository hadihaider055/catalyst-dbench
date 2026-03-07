import { useState } from "react";
import { Database, History, Bookmark } from "lucide-react";
import { cn } from "@/lib/utils";
import type { SavedConnection } from "@/lib/types";
import ConnectionDialog from "../ConnectionDialog/index";
import ConnectionsPanel from "./ConnectionsPanel";
import HistoryPanel from "./HistoryPanel";
import SavedQueriesPanel from "./SavedQueriesPanel";

type SidebarMode = "connections" | "history" | "saved";

export default function Sidebar() {
  const [mode, setMode] = useState<SidebarMode>("connections");
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [connectingConn, setConnectingConn] = useState<SavedConnection | null>(null);
  const [editingConn, setEditingConn] = useState<SavedConnection | null>(null);

  const tab = (m: SidebarMode, icon: React.ReactNode, label: string) => (
    <button
      className={cn(
        "flex-1 flex items-center justify-center gap-1.5 py-2 text-2xs font-semibold uppercase tracking-widest transition-colors",
        mode === m
          ? "text-accent border-b-2 border-accent"
          : "text-text-muted hover:text-text-secondary",
      )}
      onClick={() => setMode(m)}
    >
      {icon} {label}
    </button>
  );

  return (
    <div className="h-full flex flex-col bg-surface-raised border-r border-surface-border overflow-hidden">
      <div className="flex items-center border-b border-surface-border flex-shrink-0">
        {tab("connections", <Database size={10} />, "DB")}
        {tab("saved", <Bookmark size={10} />, "Saved")}
        {tab("history", <History size={10} />, "History")}
      </div>

      {mode === "connections" && (
        <ConnectionsPanel
          onAddDialog={() => setShowAddDialog(true)}
          onConnect={(conn) => setConnectingConn(conn)}
          onEdit={(conn) => setEditingConn(conn)}
        />
      )}
      {mode === "saved" && <SavedQueriesPanel />}
      {mode === "history" && <HistoryPanel />}

      {showAddDialog && <ConnectionDialog onClose={() => setShowAddDialog(false)} />}
      {connectingConn && (
        <ConnectionDialog
          existing={connectingConn}
          reconnect
          onClose={() => setConnectingConn(null)}
        />
      )}
      {editingConn && (
        <ConnectionDialog existing={editingConn} onClose={() => setEditingConn(null)} />
      )}
    </div>
  );
}
