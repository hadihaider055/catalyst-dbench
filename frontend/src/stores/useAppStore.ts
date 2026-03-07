import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { ConnectionInfo, DatabaseSchema, HistoryEntry, QueryTab, SavedConnection, SavedQuery } from "@/lib/types";
import { addConnection, getSchema } from "@/lib/commands";
import { generateId } from "@/lib/utils";

interface AppState {
  // Active connections (from Tauri backend)
  activeConnections: ConnectionInfo[];
  // Saved connection configs (local, no passwords)
  savedConnections: SavedConnection[];
  // Schema cache per connection id
  schemas: Record<string, DatabaseSchema | "loading" | { error: string }>;
  // Query history (newest first, max 500)
  history: HistoryEntry[];
  // Query editor tabs
  tabs: QueryTab[];
  activeTabId: string | null;
  // UI state
  sidebarWidth: number;
  sidebarCollapsed: boolean;
  selectedConnectionId: string | null;
  editorFontSize: number;
  theme: "dark" | "light";
  zoom: number;

  // Actions
  setActiveConnections: (conns: ConnectionInfo[]) => void;
  addActiveConnection: (conn: ConnectionInfo) => void;
  removeActiveConnection: (id: string) => void;
  setSavedConnections: (conns: SavedConnection[]) => void;
  upsertSavedConnection: (conn: SavedConnection) => void;
  removeSavedConnection: (id: string) => void;

  /** Add an entry to query history (newest first, capped at 500). */
  addToHistory: (entry: HistoryEntry) => void;
  /** Clear all history. */
  clearHistory: () => void;

  // Saved queries
  savedQueries: SavedQuery[];
  saveQuery: (name: string, sql: string, dbType?: SavedQuery["db_type"]) => void;
  deleteSavedQuery: (id: string) => void;

  /** Connect to a saved connection (calls Tauri IPC). Password required for auth-enabled DBs. */
  connectSaved: (conn: SavedConnection, password?: string) => Promise<ConnectionInfo>;
  /** Fetch and cache the schema for an active connection. */
  loadSchema: (connectionId: string) => Promise<void>;

  openTab: (connectionId: string, connectionName: string, dbType: import("@/lib/types").DatabaseType, title?: string) => string;
  closeTab: (tabId: string) => void;
  closeAllTabs: () => void;
  setActiveTab: (tabId: string) => void;
  updateTab: (tabId: string, patch: Partial<QueryTab>) => void;

  setSidebarWidth: (w: number) => void;
  toggleSidebar: () => void;
  setSelectedConnection: (id: string | null) => void;
  setEditorFontSize: (size: number) => void;
  toggleTheme: () => void;
  setZoom: (z: number) => void;
}

export const useAppStore = create<AppState>()(persist((set, get) => ({
  activeConnections: [],
  savedConnections: [],
  schemas: {},
  history: [],
  savedQueries: [],
  tabs: [],
  activeTabId: null,
  sidebarWidth: 260,
  sidebarCollapsed: false,
  selectedConnectionId: null,
  editorFontSize: 13,
  theme: "dark" as const,
  zoom: 1,

  setActiveConnections: (conns) => set({ activeConnections: conns }),
  addActiveConnection: (conn) =>
    set((s) => ({ activeConnections: [...s.activeConnections.filter((c) => c.id !== conn.id), conn] })),
  removeActiveConnection: (id) =>
    set((s) => ({
      activeConnections: s.activeConnections.filter((c) => c.id !== id),
      schemas: Object.fromEntries(Object.entries(s.schemas).filter(([k]) => k !== id)),
      tabs: s.tabs.map((t) =>
        t.connection_id === id
          ? { ...t, result: undefined, error: "Disconnected. Reconnect to run queries.", running: false }
          : t
      ),
    })),

  setSavedConnections: (conns) => set({ savedConnections: conns }),
  upsertSavedConnection: (conn) =>
    set((s) => ({
      savedConnections: s.savedConnections.some((c) => c.id === conn.id)
        ? s.savedConnections.map((c) => (c.id === conn.id ? conn : c))
        : [...s.savedConnections, conn],
    })),
  removeSavedConnection: (id) =>
    set((s) => ({ savedConnections: s.savedConnections.filter((c) => c.id !== id) })),

  addToHistory: (entry) =>
    set((s) => ({ history: [entry, ...s.history].slice(0, 500) })),
  clearHistory: () => set({ history: [] }),

  saveQuery: (name, sql, dbType) =>
    set((s) => ({
      savedQueries: [
        { id: generateId(), name, sql, db_type: dbType, created_at: Date.now() },
        ...s.savedQueries,
      ],
    })),
  deleteSavedQuery: (id) =>
    set((s) => ({ savedQueries: s.savedQueries.filter((q) => q.id !== id) })),

  connectSaved: async (conn, password) => {
    const result = await addConnection({
      name: conn.name,
      db_type: conn.db_type,
      host: conn.host,
      port: conn.port,
      database: conn.database,
      username: conn.username,
      password,
      tls_enabled: conn.tls_enabled,
      read_only: conn.read_only,
    });
    const info = result.info;
    // Update saved connection to use backend UUID so isActive() and loadSchema() work correctly
    if (conn.id !== info.id) {
      get().removeSavedConnection(conn.id);
      get().upsertSavedConnection({ ...conn, id: info.id });
    }
    set((s) => ({ activeConnections: [...s.activeConnections.filter((c) => c.id !== info.id), info] }));
    return info;
  },

  loadSchema: async (connectionId) => {
    if (get().schemas[connectionId] === "loading") return;
    set((s) => ({ schemas: { ...s.schemas, [connectionId]: "loading" } }));
    try {
      const schema = await getSchema(connectionId);
      set((s) => ({ schemas: { ...s.schemas, [connectionId]: schema } }));
    } catch (e) {
      const error = String(e);
      console.error("loadSchema failed:", connectionId, error);
      set((s) => ({ schemas: { ...s.schemas, [connectionId]: { error } } }));
    }
  },

  openTab: (connectionId, connectionName, dbType, title) => {
    const id = generateId();
    const tab: QueryTab = {
      id,
      connection_id: connectionId,
      connection_name: connectionName,
      db_type: dbType,
      title: title ?? `Query ${get().tabs.length + 1}`,
      sql: "",
      running: false,
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeTabId: id }));
    return id;
  },

  closeTab: (tabId) =>
    set((s) => {
      const idx = s.tabs.findIndex((t) => t.id === tabId);
      const tabs = s.tabs.filter((t) => t.id !== tabId);
      const activeTabId =
        s.activeTabId === tabId
          ? tabs[Math.max(0, idx - 1)]?.id ?? null
          : s.activeTabId;
      return { tabs, activeTabId };
    }),

  closeAllTabs: () => set({ tabs: [], activeTabId: null }),

  setActiveTab: (tabId) => set({ activeTabId: tabId }),

  updateTab: (tabId, patch) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, ...patch } : t)),
    })),

  setSidebarWidth: (w) => set({ sidebarWidth: Math.max(180, Math.min(480, w)) }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  setSelectedConnection: (id) => set({ selectedConnectionId: id }),
  setEditorFontSize: (size) => set({ editorFontSize: Math.max(10, Math.min(24, size)) }),
  toggleTheme: () => set((s) => ({ theme: s.theme === "dark" ? "light" : "dark" })),
  setZoom: (z) => set({ zoom: Math.max(0.5, Math.min(2, Math.round(z * 10) / 10)) }),
}), {
  name: "catalyst-dbench",
  // Only persist config/history — not runtime state
  partialize: (s) => ({
    savedConnections: s.savedConnections,
    history: s.history,
    savedQueries: s.savedQueries,
    sidebarWidth: s.sidebarWidth,
    sidebarCollapsed: s.sidebarCollapsed,
    editorFontSize: s.editorFontSize,
    theme: s.theme,
    zoom: s.zoom,
  }),
}));
