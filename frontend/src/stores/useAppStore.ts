import { create } from "zustand";
import type { ConnectionInfo, QueryTab, SavedConnection } from "@/lib/types";
import { generateId } from "@/lib/utils";

interface AppState {
  // Active connections (from Tauri backend)
  activeConnections: ConnectionInfo[];
  // Saved connection configs (local, no passwords)
  savedConnections: SavedConnection[];
  // Query editor tabs
  tabs: QueryTab[];
  activeTabId: string | null;
  // UI state
  sidebarWidth: number;
  sidebarCollapsed: boolean;
  selectedConnectionId: string | null;

  // Actions
  setActiveConnections: (conns: ConnectionInfo[]) => void;
  addActiveConnection: (conn: ConnectionInfo) => void;
  removeActiveConnection: (id: string) => void;
  setSavedConnections: (conns: SavedConnection[]) => void;
  upsertSavedConnection: (conn: SavedConnection) => void;
  removeSavedConnection: (id: string) => void;

  openTab: (connectionId: string, connectionName: string, dbType: import("@/lib/types").DatabaseType) => string;
  closeTab: (tabId: string) => void;
  setActiveTab: (tabId: string) => void;
  updateTab: (tabId: string, patch: Partial<QueryTab>) => void;

  setSidebarWidth: (w: number) => void;
  toggleSidebar: () => void;
  setSelectedConnection: (id: string | null) => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  activeConnections: [],
  savedConnections: [],
  tabs: [],
  activeTabId: null,
  sidebarWidth: 260,
  sidebarCollapsed: false,
  selectedConnectionId: null,

  setActiveConnections: (conns) => set({ activeConnections: conns }),
  addActiveConnection: (conn) =>
    set((s) => ({ activeConnections: [...s.activeConnections.filter((c) => c.id !== conn.id), conn] })),
  removeActiveConnection: (id) =>
    set((s) => ({ activeConnections: s.activeConnections.filter((c) => c.id !== id) })),

  setSavedConnections: (conns) => set({ savedConnections: conns }),
  upsertSavedConnection: (conn) =>
    set((s) => ({
      savedConnections: s.savedConnections.some((c) => c.id === conn.id)
        ? s.savedConnections.map((c) => (c.id === conn.id ? conn : c))
        : [...s.savedConnections, conn],
    })),
  removeSavedConnection: (id) =>
    set((s) => ({ savedConnections: s.savedConnections.filter((c) => c.id !== id) })),

  openTab: (connectionId, connectionName, dbType) => {
    const id = generateId();
    const tab: QueryTab = {
      id,
      connection_id: connectionId,
      connection_name: connectionName,
      db_type: dbType,
      title: `Query ${get().tabs.length + 1}`,
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

  setActiveTab: (tabId) => set({ activeTabId: tabId }),

  updateTab: (tabId, patch) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, ...patch } : t)),
    })),

  setSidebarWidth: (w) => set({ sidebarWidth: Math.max(180, Math.min(480, w)) }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  setSelectedConnection: (id) => set({ selectedConnectionId: id }),
}));
