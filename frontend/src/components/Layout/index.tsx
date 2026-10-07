import { useState, useRef, useCallback, useEffect } from "react";

// Lucide icons
import {
  Sun,
  Moon,
  ZoomIn,
  ZoomOut,
  PanelLeftOpen,
  PanelLeftClose,
  PlusCircle,
  Database,
  X,
} from "lucide-react";

// Components
import Sidebar from "../Sidebar/index";
import QueryEditor from "../QueryEditor/index";
import ResultsGrid from "../ResultsGrid/index";
import ERDiagram from "../ERDiagram/index";
import StatusBar from "../StatusBar/index";
import ConnectionDialog from "../ConnectionDialog/index";
import ImportDialog from "../ImportDialog/index";
import TabButton from "./TabButton";
import Welcome from "./Welcome";
import MonacoEditor from "@monaco-editor/react";
import ToastContainer from "../Toast/index";

// Utils
import { useAppStore } from "@/stores/useAppStore";
import { cn } from "@/lib/utils";
import { saveToDisk } from "../ResultsGrid/helpers";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import type { QueryTab } from "@/lib/types";

// ── Menu bar ──────────────────────────────────────────────────────────────────

interface MenuItem {
  label: string;
  shortcut?: string;
  onClick: () => void;
  disabled?: boolean;
  separator?: never;
}
interface MenuSep { separator: true; label?: never; shortcut?: never; onClick?: never; disabled?: never; }
type MenuEntry = MenuItem | MenuSep;

function MenuDropdown({ label, items }: { label: string; items: MenuEntry[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        className={cn(
          "px-2.5 py-0.5 text-xs rounded transition-colors select-none",
          open
            ? "bg-surface-overlay text-text-primary"
            : "text-text-secondary hover:text-text-primary hover:bg-surface-overlay",
        )}
        onClick={() => setOpen((v) => !v)}
      >
        {label}
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-0.5 z-[200] bg-surface-raised border border-surface-border rounded shadow-2xl py-1 min-w-[200px]">
          {items.map((item, i) => {
            if ("separator" in item && item.separator) {
              return <div key={i} className="my-1 border-t border-surface-border" />;
            }
            const mi = item as MenuItem;
            return (
              <button
                key={i}
                disabled={mi.disabled}
                className="flex items-center justify-between gap-4 w-full px-3 py-1.5 text-xs text-left text-text-secondary hover:bg-surface-overlay hover:text-text-primary disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                onClick={() => { setOpen(false); mi.onClick(); }}
              >
                <span>{mi.label}</span>
                {mi.shortcut && (
                  <span className="text-text-muted font-mono text-[10px]">{mi.shortcut}</span>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Keyboard shortcuts modal ───────────────────────────────────────────────────

function ShortcutsModal({ onClose }: { onClose: () => void }) {
  const shortcuts = [
    { key: "⌘ Enter / Ctrl+Enter", desc: "Run query" },
    { key: "⌘ Shift+Enter", desc: "Run all (batch)" },
    { key: "⌘ T / Ctrl+T", desc: "New query tab" },
    { key: "⌘ W / Ctrl+W", desc: "Close active tab" },
    { key: "⌘ Shift+W", desc: "Close all tabs" },
    { key: "⌘ B / Ctrl+B", desc: "Toggle sidebar" },
    { key: "⌘ + / Ctrl+=", desc: "Zoom in" },
    { key: "⌘ - / Ctrl+-", desc: "Zoom out" },
    { key: "⌘ 0 / Ctrl+0", desc: "Reset zoom" },
    { key: "⌘ / / Ctrl+/", desc: "Toggle keyboard shortcuts" },
    { key: "Shift+Alt+F", desc: "Format query" },
    { key: "Escape", desc: "Close modal" },
  ];
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" onClick={onClose}>
      <div className="bg-surface-raised border border-surface-border rounded-lg shadow-2xl p-5 w-[420px]" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold text-text-primary">Keyboard Shortcuts</h2>
          <button className="text-text-muted hover:text-text-primary" onClick={onClose}><X size={14} /></button>
        </div>
        <div className="space-y-1">
          {shortcuts.map(({ key, desc }) => (
            <div key={key} className="flex items-center justify-between py-1 border-b border-surface-border/50">
              <span className="text-xs text-text-secondary">{desc}</span>
              <kbd className="text-[10px] font-mono bg-surface border border-surface-border rounded px-1.5 py-0.5 text-text-muted">{key}</kbd>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ── DDL Viewer ───────────────────────────────────────────────────────────────

function DdlViewer({ tab }: { tab: QueryTab }) {
  const { theme } = useAppStore();
  const monacoTheme = theme === "dark" ? "vs-dark" : "vs";
  return (
    <div className="flex-1 flex flex-col h-full bg-surface">
      <div className="flex items-center px-3 py-1.5 bg-surface-raised border-b border-surface-border text-xs text-text-muted flex-shrink-0">
        <span className="font-medium text-text-secondary mr-2">{tab.ddl_object}</span>
        <span>DDL</span>
        <span className="ml-auto text-text-muted">{tab.connection_name}</span>
      </div>
      <div className="flex-1 min-h-0">
        <MonacoEditor
          height="100%"
          language="sql"
          value={tab.ddl_content ?? ""}
          theme={monacoTheme}
          options={{
            readOnly: true,
            fontSize: 13,
            fontFamily: "'JetBrains Mono', 'Fira Code', Menlo, monospace",
            fontLigatures: true,
            lineHeight: 20,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            wordWrap: "on",
            automaticLayout: true,
            padding: { top: 12, bottom: 12 },
          }}
        />
      </div>
    </div>
  );
}

// ── Layout ────────────────────────────────────────────────────────────────────

export default function Layout() {
  const {
    sidebarWidth,
    sidebarCollapsed,
    setSidebarWidth,
    toggleSidebar,
    tabs,
    activeTabId,
    activeConnections,
    openTab,
    closeTab,
    closeAllTabs,
    schemas,
    theme,
    toggleTheme,
    zoom,
    setZoom,
    addToast,
  } = useAppStore();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [queryPanelHeight, setQueryPanelHeight] = useState(300);
  const isDraggingSidebar = useRef(false);
  const isDraggingResults = useRef(false);

  const activeTab = tabs.find((t) => t.id === activeTabId);
  const activeConn = activeConnections.find((c) => c.id === activeTab?.connection_id);
  const SQL_DB_TYPES = new Set(["postgres", "mysql", "sqlite", "cockroachdb", "mssql", "oracle", "clickhouse", "cassandra", "surrealdb", "dynamodb", "elasticsearch"]);
  const isSqlActive = activeConn ? SQL_DB_TYPES.has(activeConn.db_type) : false;

  // Apply theme synchronously on every render to avoid flash
  document.documentElement.setAttribute("data-theme", theme);


  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      // Escape closes any open modal
      if (e.key === "Escape") {
        if (showShortcuts) { setShowShortcuts(false); e.preventDefault(); }
        return;
      }
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      if (e.key === "=" || e.key === "+") { e.preventDefault(); setZoom(zoom + 0.1); }
      else if (e.key === "-") { e.preventDefault(); setZoom(zoom - 0.1); }
      else if (e.key === "0") { e.preventDefault(); setZoom(1); }
      else if (e.key === "?" || e.key === "/") { e.preventDefault(); setShowShortcuts((v) => !v); }
      else if (e.key === "t" || e.key === "T") {
        e.preventDefault();
        const c = activeConnections[0];
        if (c) openTab(c.id, c.host, c.db_type);
      } else if (e.key === "w" || e.key === "W") {
        e.preventDefault();
        if (e.shiftKey) { closeAllTabs(); }
        else if (activeTabId) { closeTab(activeTabId); }
      } else if (e.key === "b" || e.key === "B") {
        e.preventDefault();
        toggleSidebar();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [zoom, setZoom, activeConnections, openTab, closeTab, closeAllTabs, activeTabId, toggleSidebar, showShortcuts]);

  const exportSchemaAsSql = async () => {
    if (!activeTab) return;
    const schema = schemas[activeTab.connection_id];
    if (!schema || schema === "loading" || "error" in schema) {
      addToast("Schema is still loading — try again in a moment.", "info");
      return;
    }
    const lines: string[] = [`-- Schema export: ${schema.name} (${schema.db_type})\n-- Generated by Catalyst DBench\n`];
    for (const obj of schema.objects) {
      if (obj.kind !== "table") continue;
      const cols = (obj.columns ?? []) as Array<{ name: string; native_type: string; nullable: boolean; is_primary_key: boolean }>;
      const colDefs = cols.map((c) => {
        const parts = [`  \`${c.name}\` ${c.native_type}`];
        if (!c.nullable) parts.push("NOT NULL");
        if (c.is_primary_key) parts.push("PRIMARY KEY");
        return parts.join(" ");
      });
      const fqn = (obj.schema as string | undefined) ? `\`${obj.schema}\`.\`${obj.name as string}\`` : `\`${obj.name as string}\``;
      lines.push(`CREATE TABLE IF NOT EXISTS ${fqn} (\n${colDefs.join(",\n")}\n);\n`);
    }
    await saveToDisk(lines.join("\n"), `schema-${schema.name}`, "sql", `Schema for ${schema.name} exported`);
  };

  const importSql = async () => {
    const path = await openFileDialog({
      multiple: false,
      filters: [{ name: "SQL Files", extensions: ["sql", "txt"] }],
    });
    if (!path || typeof path !== "string") return;
    const content = await readTextFile(path);
    if (!activeTab) {
      // Open a new tab if there's an active connection
      const c = activeConnections[0];
      if (!c) return;
      const tabId = openTab(c.id, c.host, c.db_type, path.split("/").pop()?.replace(/\.sql$/i, "") ?? "Import");
      useAppStore.setState((s) => ({ tabs: s.tabs.map((t) => t.id === tabId ? { ...t, sql: content } : t) }));
    } else {
      useAppStore.setState((s) => ({ tabs: s.tabs.map((t) => t.id === activeTab.id ? { ...t, sql: content } : t) }));
    }
  };

  const onSidebarMouseDown = useCallback(() => {
    isDraggingSidebar.current = true;
    const onMove = (e: MouseEvent) => {
      if (isDraggingSidebar.current) setSidebarWidth(e.clientX);
    };
    const onUp = () => {
      isDraggingSidebar.current = false;
      window.removeEventListener("mousemove", onMove);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp, { once: true });
  }, [setSidebarWidth]);

  const onResultsMouseDown = useCallback(
    (e: React.MouseEvent) => {
      isDraggingResults.current = true;
      const startY = e.clientY;
      const startH = queryPanelHeight;
      const onMove = (ev: MouseEvent) => {
        if (isDraggingResults.current)
          setQueryPanelHeight(
            Math.max(100, Math.min(600, startH + (startY - ev.clientY))),
          );
      };
      const onUp = () => {
        isDraggingResults.current = false;
        window.removeEventListener("mousemove", onMove);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp, { once: true });
    },
    [queryPanelHeight],
  );

  const fileMenuItems: MenuEntry[] = [
    { label: "New Connection…", shortcut: "⌘N", onClick: () => setShowAddDialog(true) },
    {
      label: "New Query Tab",
      shortcut: "⌘T",
      disabled: activeConnections.length === 0,
      onClick: () => { const c = activeConnections[0]; if (c) openTab(c.id, c.host, c.db_type); },
    },
    { separator: true },
    {
      label: "Open SQL File…",
      disabled: !isSqlActive,
      onClick: importSql,
    },
    {
      label: "Import CSV / JSON…",
      disabled: !activeTab || !isSqlActive,
      onClick: () => setShowImportDialog(true),
    },
    {
      label: "Export Schema as SQL…",
      shortcut: "⌘⇧E",
      disabled: !activeTab || !isSqlActive,
      onClick: exportSchemaAsSql,
    },
    { separator: true },
    {
      label: "Close Tab",
      shortcut: "⌘W",
      disabled: !activeTabId,
      onClick: () => { if (activeTabId) closeTab(activeTabId); },
    },
    {
      label: "Close All Tabs",
      shortcut: "⌘⇧W",
      disabled: tabs.length === 0,
      onClick: closeAllTabs,
    },
  ];

  const viewMenuItems: MenuEntry[] = [
    {
      label: sidebarCollapsed ? "Show Sidebar" : "Hide Sidebar",
      shortcut: "⌘B",
      onClick: toggleSidebar,
    },
    { separator: true },
    { label: "Zoom In", shortcut: "⌘+", onClick: () => setZoom(zoom + 0.1) },
    { label: "Zoom Out", shortcut: "⌘−", onClick: () => setZoom(zoom - 0.1) },
    { label: "Reset Zoom", shortcut: "⌘0", onClick: () => setZoom(1) },
    { separator: true },
    {
      label: theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode",
      onClick: toggleTheme,
    },
  ];

  const toolsMenuItems: MenuEntry[] = [
    { label: "Keyboard Shortcuts…", shortcut: "⌘/", onClick: () => setShowShortcuts(true) },
  ];

  return (
    <div
      className="flex flex-col bg-surface overflow-hidden"
      style={{
        width: `${100 / zoom}vw`,
        height: `${100 / zoom}vh`,
        transform: `scale(${zoom})`,
        transformOrigin: "0 0",
      }}
    >
      {/* Menu bar row */}
      <div
        className="flex items-center h-8 bg-surface-raised border-b border-surface-border px-1.5 gap-0.5 flex-shrink-0"
        data-tauri-drag-region
      >
        {/* Sidebar toggle */}
        <button
          className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors flex-shrink-0"
          onClick={toggleSidebar}
          title={sidebarCollapsed ? "Show sidebar (⌘B)" : "Hide sidebar (⌘B)"}
        >
          {sidebarCollapsed ? <PanelLeftOpen size={13} /> : <PanelLeftClose size={13} />}
        </button>

        <span className="text-xs font-semibold text-text-secondary tracking-wide select-none flex-shrink-0 px-1 mr-0.5">
          DBench
        </span>

        {/* Menu dropdowns */}
        <MenuDropdown label="File" items={fileMenuItems} />
        <MenuDropdown label="View" items={viewMenuItems} />
        <MenuDropdown label="Tools" items={toolsMenuItems} />

        {/* Drag region */}
        <div className="flex-1 min-w-0" data-tauri-drag-region />

        {/* Right controls */}
        <div className="flex items-center gap-0.5 flex-shrink-0">
          <button
            className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
            onClick={() => setZoom(zoom - 0.1)}
            title="Zoom out (⌘−)"
          >
            <ZoomOut size={12} />
          </button>
          <button
            className="text-2xs text-text-muted w-8 text-center hover:text-text-primary cursor-pointer"
            onClick={() => setZoom(1)}
            title="Reset zoom (⌘0)"
          >
            {Math.round(zoom * 100)}%
          </button>
          <button
            className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
            onClick={() => setZoom(zoom + 0.1)}
            title="Zoom in (⌘+)"
          >
            <ZoomIn size={12} />
          </button>
          <div className="w-px h-4 bg-surface-border mx-0.5" />
          <button
            className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
            onClick={toggleTheme}
            title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
          >
            {theme === "dark" ? <Sun size={13} /> : <Moon size={13} />}
          </button>
        </div>
      </div>

      {/* Tab bar row */}
      <div className="flex items-center h-8 bg-surface-raised border-b border-surface-border px-1.5 gap-0.5 flex-shrink-0">
        {/* New connection + new query buttons */}
        <button
          className="flex items-center gap-1 px-2 py-0.5 text-2xs text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded border border-surface-border transition-colors flex-shrink-0"
          onClick={() => setShowAddDialog(true)}
          title="New connection"
        >
          <Database size={10} /> New
        </button>
        {activeConnections.length > 0 && (
          <button
            className="flex items-center gap-1 px-2 py-0.5 text-2xs text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded border border-surface-border transition-colors flex-shrink-0"
            onClick={() => { const c = activeConnections[0]; openTab(c.id, c.host, c.db_type); }}
            title="New query tab (⌘T)"
          >
            <PlusCircle size={10} /> Query
          </button>
        )}

        <div className="w-px h-4 bg-surface-border mx-0.5 flex-shrink-0" />

        {/* Tabs */}
        <div className="flex items-end gap-0.5 overflow-x-auto flex-1 min-w-0 h-full">
          {tabs.map((tab) => (
            <TabButton key={tab.id} tab={tab} />
          ))}
          {tabs.length === 0 && (
            <span className="text-xs text-text-muted px-2 self-center select-none">
              No open tabs
            </span>
          )}
        </div>

        {/* Close all */}
        {tabs.length > 0 && (
          <button
            className="flex items-center gap-1 px-2 py-0.5 text-2xs text-text-muted hover:text-red-400 hover:bg-surface-overlay rounded border border-surface-border transition-colors flex-shrink-0 ml-0.5"
            onClick={closeAllTabs}
            title="Close all tabs (⌘⇧W)"
          >
            <X size={10} /> All
          </button>
        )}
      </div>

      {showShortcuts && <ShortcutsModal onClose={() => setShowShortcuts(false)} />}

      {showAddDialog && (
        <ConnectionDialog onClose={() => setShowAddDialog(false)} />
      )}

      {showImportDialog && activeTab && (
        <ImportDialog
          connectionId={activeTab.connection_id}
          onClose={() => setShowImportDialog(false)}
          onSuccess={(n) => {
            setShowImportDialog(false);
            console.info(`Imported ${n} rows`);
          }}
        />
      )}

      {/* Main content */}
      <div className="flex flex-1 min-h-0">
        {!sidebarCollapsed && (
          <>
            <div
              style={{ width: sidebarWidth }}
              className="flex-shrink-0 min-w-0"
            >
              <Sidebar />
            </div>
            <div
              className="w-1 bg-surface-border hover:bg-accent cursor-col-resize flex-shrink-0 transition-colors"
              onMouseDown={onSidebarMouseDown}
            />
          </>
        )}

        <div className="flex-1 flex flex-col min-w-0">
          {activeTab ? (
            activeTab.kind === "er_diagram" ? (
              <div className="flex-1 min-h-0">
                <ERDiagram connectionId={activeTab.er_connection_id ?? activeTab.connection_id} />
              </div>
            ) : activeTab.kind === "ddl" ? (
              <DdlViewer tab={activeTab} />
            ) : (
              <>
                <div
                  className="flex-shrink-0"
                  style={{ height: `calc(100% - ${queryPanelHeight}px - 5px)` }}
                >
                  <QueryEditor tab={activeTab} />
                </div>
                <div
                  className="h-1.5 bg-surface-border hover:bg-accent cursor-row-resize flex-shrink-0 flex items-center justify-center transition-colors"
                  onMouseDown={onResultsMouseDown}
                >
                  <div className="w-8 h-0.5 bg-text-muted rounded" />
                </div>
                <div
                  style={{ height: queryPanelHeight }}
                  className="flex-shrink-0"
                >
                  <ResultsGrid tab={activeTab} />
                </div>
              </>
            )
          ) : (
            <Welcome />
          )}
        </div>
      </div>

      <StatusBar />
      <ToastContainer />
    </div>
  );
}
