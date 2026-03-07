import { useState } from "react";
import { useAppStore } from "@/stores/useAppStore";
import { getCredential } from "@/lib/commands";
import { dbIcon } from "@/lib/utils";
import type { SavedConnection } from "@/lib/types";
import ConnectionDialog from "../ConnectionDialog/index";

export default function Welcome() {
  const { savedConnections, activeConnections, connectSaved, openTab } =
    useAppStore();
  const [connecting, setConnecting] = useState<string | null>(null);
  const [reconnectConn, setReconnectConn] = useState<SavedConnection | null>(
    null,
  );

  const handleConnect = async (conn: SavedConnection) => {
    const alreadyActive = activeConnections.find((c) => c.id === conn.id);
    if (alreadyActive) {
      openTab(alreadyActive.id, conn.name, conn.db_type);
      return;
    }
    setConnecting(conn.id);
    try {
      const password = await getCredential(conn.id).catch(() => null);
      const info = await connectSaved(conn, password ?? undefined);
      openTab(info.id, conn.name, conn.db_type);
    } catch {
      // Keychain had no password or connect failed — open reconnect dialog so user can enter it.
      setReconnectConn(conn);
    } finally {
      setConnecting(null);
    }
  };

  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-6 text-text-secondary">
      <div className="text-center">
        <div className="text-4xl mb-3">🗄️</div>
        <h2 className="text-lg font-semibold text-text-primary mb-1">
          Catalyst DBench
        </h2>
        <p className="text-sm">Connect to a database to start querying</p>
      </div>
      {savedConnections.length > 0 && (
        <div className="flex flex-col gap-2 w-full max-w-sm">
          <p className="text-xs text-text-muted text-center">
            Saved connections
          </p>
          {savedConnections.slice(0, 5).map((conn) => {
            const isActive = activeConnections.some((c) => c.id === conn.id);
            const isConnecting = connecting === conn.id;
            return (
              <button
                key={conn.id}
                className="flex items-center gap-2 px-3 py-2 bg-surface-raised hover:bg-surface-overlay rounded text-sm text-left transition-colors disabled:opacity-50 overflow-hidden w-full"
                onClick={() => handleConnect(conn)}
                disabled={isConnecting}
              >
                <span className="flex-shrink-0">{dbIcon(conn.db_type)}</span>
                <span className="font-medium flex-1 text-ellipsis whitespace-nowrap overflow-hidden">
                  {conn.name}
                </span>
                <span className="text-text-muted text-xs flex-shrink-0 text-ellipsis max-w-[120px] w-full whitespace-nowrap overflow-hidden text-right">
                  {isConnecting ? (
                    "…"
                  ) : isActive ? (
                    <span className="text-green-400">●</span>
                  ) : (
                    conn.host
                  )}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {reconnectConn && (
        <ConnectionDialog
          existing={reconnectConn}
          reconnect
          onClose={() => setReconnectConn(null)}
        />
      )}
    </div>
  );
}
