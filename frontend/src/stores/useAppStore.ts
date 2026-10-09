import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";
import { load, type Store } from "@tauri-apps/plugin-store";
import type { ConnectionInfo, DatabaseSchema, HistoryEntry, QueryTab, SavedConnection, SavedQuery } from "@/lib/types";
import { addConnection, closeConnection, getSchema, getCredential, storeCredential } from "@/lib/commands";
import { generateId, splitUriPassword } from "@/lib/utils";

export interface Toast {
  id: string;
  message: string;
  type: "success" | "error" | "info";
}

interface AppState {
  // Toasts
  toasts: Toast[];
  addToast: (message: string, type?: Toast["type"]) => void;
  removeToast: (id: string) => void;

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
  /** Move passwords embedded in saved connection URIs into the OS keychain. */
  secureSavedConnections: () => Promise<void>;
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
  /**
   * Switch the active database for an existing connection by reconnecting.
   * Updates all open tabs and the schema cache to use the new connection ID.
   */
  switchDatabase: (connectionId: string, database: string) => Promise<void>;
  /** Fetch and cache the schema for an active connection. */
  loadSchema: (connectionId: string) => Promise<void>;

  openTab: (connectionId: string, connectionName: string, dbType: import("@/lib/types").DatabaseType, title?: string) => string;
  openERDiagramTab: (connectionId: string, connectionName: string, dbType: import("@/lib/types").DatabaseType) => string;
  openDdlTab: (connectionId: string, connectionName: string, dbType: import("@/lib/types").DatabaseType, objectName: string, ddlContent: string) => string;
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

// Persisted state lives in a file in the app data dir, shared by `cargo tauri dev`
// (origin localhost:5173) and the built app (tauri://localhost). localStorage is
// per-origin, so connections saved in one never showed up in the other.
let fileStore: Promise<Store> | null = null;
const getFileStore = () => (fileStore ??= load("app-state.json"));
// Until the first read finishes, a write would persist the *default* state (e.g. from
// setActiveConnections at startup) over the saved file — so drop writes until loaded.
let loaded = false;

const appStateStorage: StateStorage =
  "__TAURI_INTERNALS__" in window
    ? {
        getItem: async (key) => {
          try {
            const value = await (await getFileStore()).get<string>(key);
            // One-time migration: fall back to this origin's old localStorage copy.
            return value ?? localStorage.getItem(key);
          } finally {
            loaded = true;
          }
        },
        setItem: async (key, value) => {
          if (!loaded) return;
          const store = await getFileStore();
          await store.set(key, value);
          await store.save();
          localStorage.removeItem(key); // old per-origin copy is now obsolete
        },
        removeItem: async (key) => {
          const store = await getFileStore();
          await store.delete(key);
          await store.save();
        },
      }
    : localStorage;

export const useAppStore = create<AppState>()(persist((set, get) => ({
  toasts: [],
  addToast: (message, type = "success") => {
    const id = generateId();
    set((s) => ({ toasts: [...s.toasts, { id, message, type }] }));
    setTimeout(() => get().removeToast(id), 3500);
  },
  removeToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

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
  removeActiveConnection: (id) => {
    // Close it in the backend too, or the DB session and any SSH tunnel
    // (a 127.0.0.1 port forward any local process can use) stay open.
    closeConnection(id).catch(() => {/* already closed */});
    set((s) => ({
      activeConnections: s.activeConnections.filter((c) => c.id !== id),
      schemas: Object.fromEntries(Object.entries(s.schemas).filter(([k]) => k !== id)),
      tabs: s.tabs.map((t) =>
        t.connection_id === id
          ? { ...t, result: undefined, error: "Disconnected. Reconnect to run queries.", running: false }
          : t
      ),
    }));
  },

  setSavedConnections: (conns) => set({ savedConnections: conns }),
  upsertSavedConnection: (conn) => {
    // Never persist a password inside a URI (mongodb://user:pass@…): keychain only.
    const { uri, password } = splitUriPassword(conn.host);
    if (password) {
      conn = { ...conn, host: uri };
      storeCredential(conn.id, password).catch(() => {/* user re-enters it on reconnect */});
    }
    set((s) => ({
      savedConnections: s.savedConnections.some((c) => c.id === conn.id)
        ? s.savedConnections.map((c) => (c.id === conn.id ? conn : c))
        : [...s.savedConnections, conn],
    }));
  },
  secureSavedConnections: async () => {
    for (const c of get().savedConnections) {
      const { uri, password } = splitUriPassword(c.host);
      if (!password) continue;
      // Strip from the file only once the keychain has it, so it can't be lost.
      try {
        await storeCredential(c.id, password);
      } catch {
        continue;
      }
      set((s) => ({
        savedConnections: s.savedConnections.map((x) => (x.id === c.id ? { ...x, host: uri } : x)),
      }));
    }
  },
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
      tls_ca_path: conn.tls_ca_path,
      read_only: conn.read_only,
      ssh_enabled: conn.ssh_enabled,
      ssh_host: conn.ssh_host,
      ssh_port: conn.ssh_port,
      ssh_username: conn.ssh_username,
      ssh_auth_method: conn.ssh_auth_method,
      ssh_key_path: conn.ssh_key_path,
    });
    const info = result.info;
    // Update saved connection to use backend UUID so isActive() and loadSchema() work correctly.
    // The keychain entry is keyed by id, so carry the password over to the new one.
    if (conn.id !== info.id) {
      if (password) await storeCredential(info.id, password).catch(() => {/* ignore */});
      get().removeSavedConnection(conn.id);
      get().upsertSavedConnection({ ...conn, id: info.id });
    }
    set((s) => ({ activeConnections: [...s.activeConnections.filter((c) => c.id !== info.id), info] }));
    return info;
  },

  switchDatabase: async (connectionId, database) => {
    const { savedConnections } = get();
    const saved = savedConnections.find((c) => c.id === connectionId);
    if (!saved) return;

    let password: string | undefined;
    try {
      password = (await getCredential(connectionId)) ?? undefined;
    } catch { /* no credential stored */ }

    const result = await addConnection({
      name: saved.name,
      db_type: saved.db_type,
      host: saved.host,
      port: saved.port,
      database,
      username: saved.username,
      password,
      tls_enabled: saved.tls_enabled,
      tls_ca_path: saved.tls_ca_path,
      read_only: saved.read_only,
      ssh_enabled: saved.ssh_enabled,
      ssh_host: saved.ssh_host,
      ssh_port: saved.ssh_port,
      ssh_username: saved.ssh_username,
      ssh_auth_method: saved.ssh_auth_method,
      ssh_key_path: saved.ssh_key_path,
    });

    const newInfo = result.info;
    const newId = newInfo.id;
    // The old connection (and its SSH tunnel) is replaced: close it in the backend.
    closeConnection(connectionId).catch(() => {/* already closed */});

    if (password) {
      try { await storeCredential(newId, password); } catch { /* ignore */ }
    }

    get().removeSavedConnection(connectionId);
    get().upsertSavedConnection({ ...saved, id: newId, database });

    set((s) => ({
      activeConnections: [
        ...s.activeConnections.filter((c) => c.id !== connectionId),
        newInfo,
      ],
      tabs: s.tabs.map((t) =>
        t.connection_id === connectionId
          ? { ...t, connection_id: newId, connection_name: t.connection_name }
          : t
      ),
      schemas: Object.fromEntries(
        Object.entries(s.schemas).filter(([k]) => k !== connectionId)
      ),
      selectedConnectionId: newId,
    }));
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
      kind: "query",
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeTabId: id }));
    return id;
  },

  openDdlTab: (connectionId, connectionName, dbType, objectName, ddlContent) => {
    const existing = get().tabs.find(
      (t) => t.kind === "ddl" && t.ddl_object === objectName && t.connection_id === connectionId,
    );
    if (existing) {
      set((s) => ({
        tabs: s.tabs.map((t) => t.id === existing.id ? { ...t, ddl_content: ddlContent } : t),
        activeTabId: existing.id,
      }));
      return existing.id;
    }
    const id = generateId();
    const tab: QueryTab = {
      id,
      connection_id: connectionId,
      connection_name: connectionName,
      db_type: dbType,
      title: objectName,
      sql: "",
      running: false,
      kind: "ddl",
      ddl_content: ddlContent,
      ddl_object: objectName,
    };
    set((s) => ({ tabs: [...s.tabs, tab], activeTabId: id }));
    return id;
  },

  openERDiagramTab: (connectionId, connectionName, dbType) => {
    const existing = get().tabs.find(
      (t) => t.kind === "er_diagram" && t.er_connection_id === connectionId,
    );
    if (existing) {
      set({ activeTabId: existing.id });
      return existing.id;
    }
    const id = generateId();
    const tab: QueryTab = {
      id,
      connection_id: connectionId,
      connection_name: connectionName,
      db_type: dbType,
      title: "ER Diagram",
      sql: "",
      running: false,
      kind: "er_diagram",
      er_connection_id: connectionId,
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
  storage: createJSONStorage(() => appStateStorage),
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
