import { useState } from "react";
import { Plug, PlugZap, Trash2, Eye, Copy, Database, Lock } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { cn, dbIcon, dbColor } from "@/lib/utils";
import type { SavedConnection } from "@/lib/types";
import ContextMenu, { type ContextMenuEntry } from "../ContextMenu/index";
import DangerConfirm from "./DangerConfirm";
import SchemaTree from "./SchemaTree";

export interface ConnectionItemProps {
  conn: SavedConnection;
  activeId: string | undefined;
  selected: boolean;
  expanded: boolean;
  onExpand: () => void;
  onSelect: () => void;
  onOpen: () => void;
  onConnect: () => void;
  onEdit: () => void;
}

export default function ConnectionItem({
  conn,
  activeId,
  selected,
  expanded,
  onExpand,
  onSelect,
  onOpen,
  onConnect,
  onEdit,
}: ConnectionItemProps) {
  const { removeSavedConnection, removeActiveConnection } = useAppStore();
  const active = !!activeId;
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);
  const [removeConfirm, setRemoveConfirm] = useState(false);

  const handleDisconnect = (e: React.MouseEvent) => {
    e.stopPropagation();
    removeActiveConnection(activeId ?? conn.id);
  };

  const handleRemove = () => {
    removeActiveConnection(activeId ?? conn.id);
    removeSavedConnection(conn.id);
    setRemoveConfirm(false);
  };

  const connCtxItems: ContextMenuEntry[] = [
    ...(active
      ? [
          {
            label: "Open new tab",
            icon: <Database size={11} />,
            onClick: onOpen,
          },
        ]
      : [{ label: "Connect", icon: <Plug size={11} />, onClick: onConnect }]),
    ...(active
      ? [
          {
            label: "Disconnect",
            icon: <PlugZap size={11} />,
            onClick: () => removeActiveConnection(activeId!),
          },
        ]
      : []),
    { separator: true },
    { label: "Edit connection", icon: <Eye size={11} />, onClick: onEdit },
    {
      label: "Copy host",
      icon: <Copy size={11} />,
      onClick: () => navigator.clipboard.writeText(`${conn.host}:${conn.port}`),
    },
    { separator: true },
    {
      label: "Remove connection…",
      icon: <Trash2 size={11} />,
      danger: true,
      onClick: () => setRemoveConfirm(true),
    },
  ];

  return (
    <div className={cn("group", selected && "bg-accent-muted")}>
      <div
        className={cn(
          "flex items-center gap-1.5 px-2 py-1.5 cursor-pointer hover:bg-surface-overlay transition-colors",
          selected && "bg-accent-muted",
        )}
        onClick={() => {
          onSelect();
          onExpand();
        }}
        onDoubleClick={active ? onOpen : onConnect}
        onContextMenu={(e) => {
          e.preventDefault();
          setCtxMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        <span
          className="text-sm flex-shrink-0"
          style={{ filter: `drop-shadow(0 0 3px ${dbColor(conn.db_type)})` }}
        >
          {dbIcon(conn.db_type)}
        </span>

        <span className="flex-1 text-xs text-text-primary truncate font-medium">
          {conn.name}
        </span>

        {conn.tls_enabled && (
          <Lock
            size={10}
            className="text-text-muted flex-shrink-0"
            aria-label="TLS enabled"
          />
        )}
        {conn.read_only && (
          <span className="text-2xs text-yellow-500 flex-shrink-0">RO</span>
        )}
        <div
          className={cn(
            "w-1.5 h-1.5 rounded-full flex-shrink-0",
            active ? "bg-green-400" : "bg-text-muted",
          )}
        />

        <div className="hidden group-hover:flex items-center gap-1 flex-shrink-0">
          {active ? (
            <button
              className="text-text-muted hover:text-red-400 p-0.5"
              onClick={handleDisconnect}
              title="Disconnect"
            >
              <PlugZap size={11} />
            </button>
          ) : (
            <button
              className="text-text-muted hover:text-green-400 p-0.5"
              onClick={(e) => {
                e.stopPropagation();
                onConnect();
              }}
              title="Connect"
            >
              <Plug size={11} />
            </button>
          )}
          <button
            className="text-text-muted hover:text-text-primary p-0.5"
            onClick={(e) => {
              e.stopPropagation();
              onEdit();
            }}
            title="Edit connection"
          >
            <Eye size={11} />
          </button>
          <button
            className="text-text-muted hover:text-red-500 p-0.5"
            onClick={(e) => {
              e.stopPropagation();
              setRemoveConfirm(true);
            }}
            title="Remove"
          >
            <Trash2 size={11} />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="pl-6">
          {activeId ? (
            <SchemaTree connectionId={activeId} />
          ) : (
            <div className="px-2 py-1.5 text-2xs text-text-muted italic">
              Not connected — double-click to connect
            </div>
          )}
        </div>
      )}

      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          items={connCtxItems}
          onClose={() => setCtxMenu(null)}
        />
      )}

      {removeConfirm && (
        <DangerConfirm
          title={`Remove "${conn.name}"?`}
          sql="This will remove the saved connection config. The database itself will not be affected."
          confirmLabel="Remove"
          onConfirm={handleRemove}
          onCancel={() => setRemoveConfirm(false)}
        />
      )}
    </div>
  );
}
