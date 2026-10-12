import { Shield, Database, Lock } from "lucide-react";
import DbIcon from "@/components/DbIcon";
import { useAppStore } from "@/stores/useAppStore";

export default function StatusBar() {
  const { activeConnections, selectedConnectionId, tabs, activeTabId } = useAppStore();

  const activeTab = tabs.find((t) => t.id === activeTabId);
  const conn = activeConnections.find(
    (c) => c.id === (activeTab?.connection_id ?? selectedConnectionId)
  );

  return (
    <div className="flex items-center gap-3 px-3 h-6 bg-accent text-white text-2xs flex-shrink-0 select-none overflow-hidden">
      {conn ? (
        <>
          <span className="flex items-center gap-1">
            <DbIcon type={conn.db_type} />
            <span className="font-medium">{conn.host}:{conn.port}</span>
          </span>
          <span className="text-blue-200">{conn.database}</span>
          {conn.tls_active && (
            <span className="flex items-center gap-0.5 text-green-300">
              <Lock size={9} /> TLS
            </span>
          )}
          {conn.server_version && (
            <span className="text-blue-200">{conn.server_version}</span>
          )}
        </>
      ) : (
        <span className="flex items-center gap-1 text-blue-200">
          <Database size={10} />
          No active connection
        </span>
      )}

      <div className="flex-1" />

      <span className="flex items-center gap-1 text-green-300">
        <Shield size={9} />
        Encrypted
      </span>

      <span className="text-blue-200">
        {activeConnections.length} {activeConnections.length === 1 ? "connection" : "connections"}
      </span>
    </div>
  );
}
