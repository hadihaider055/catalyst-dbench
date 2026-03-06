import { useState, useRef, useCallback } from "react";
import { useAppStore } from "@/stores/useAppStore";
import Sidebar from "./Sidebar";
import QueryEditor from "./QueryEditor";
import ResultsGrid from "./ResultsGrid";
import StatusBar from "./StatusBar";
import { cn } from "@/lib/utils";

export default function Layout() {
  const { sidebarWidth, sidebarCollapsed, setSidebarWidth, tabs, activeTabId } =
    useAppStore();
  const [queryPanelHeight, setQueryPanelHeight] = useState(300);
  const isDraggingSidebar = useRef(false);
  const isDraggingResults = useRef(false);

  const activeTab = tabs.find((t) => t.id === activeTabId);

  // Sidebar resize
  const onSidebarMouseDown = useCallback(() => {
    isDraggingSidebar.current = true;
    const onMove = (e: MouseEvent) => {
      if (isDraggingSidebar.current) setSidebarWidth(e.clientX);
    };
    const onUp = () => { isDraggingSidebar.current = false; window.removeEventListener("mousemove", onMove); };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp, { once: true });
  }, [setSidebarWidth]);

  // Results panel resize
  const onResultsMouseDown = useCallback((e: React.MouseEvent) => {
    isDraggingResults.current = true;
    const startY = e.clientY;
    const startH = queryPanelHeight;
    const onMove = (ev: MouseEvent) => {
      if (isDraggingResults.current)
        setQueryPanelHeight(Math.max(100, Math.min(600, startH + (startY - ev.clientY))));
    };
    const onUp = () => { isDraggingResults.current = false; window.removeEventListener("mousemove", onMove); };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp, { once: true });
  }, [queryPanelHeight]);

  return (
    <div className="flex flex-col h-screen bg-surface overflow-hidden">
      {/* Title bar */}
      <div
        className="flex items-center h-9 bg-surface-raised border-b border-surface-border px-3 gap-3 flex-shrink-0"
        data-tauri-drag-region
      >
        <span className="text-xs font-semibold text-text-secondary tracking-wide select-none">
          CATALYST DBENCH
        </span>
        <div className="flex-1" data-tauri-drag-region />
        {/* Tab bar */}
        <div className="flex items-center gap-0.5 overflow-x-auto">
          {tabs.map((tab) => (
            <TabButton key={tab.id} tab={tab} />
          ))}
          {tabs.length === 0 && (
            <span className="text-xs text-text-muted px-2">No open tabs</span>
          )}
        </div>
        <div className="flex-1" data-tauri-drag-region />
      </div>

      {/* Main content */}
      <div className="flex flex-1 min-h-0">
        {/* Sidebar */}
        {!sidebarCollapsed && (
          <>
            <div style={{ width: sidebarWidth }} className="flex-shrink-0 min-w-0">
              <Sidebar />
            </div>
            {/* Sidebar resize handle */}
            <div
              className="w-1 bg-surface-border hover:bg-accent cursor-col-resize flex-shrink-0 transition-colors"
              onMouseDown={onSidebarMouseDown}
            />
          </>
        )}

        {/* Editor + Results */}
        <div className="flex-1 flex flex-col min-w-0">
          {activeTab ? (
            <>
              {/* Query editor */}
              <div className="flex-shrink-0" style={{ height: `calc(100% - ${queryPanelHeight}px - 5px)` }}>
                <QueryEditor tab={activeTab} />
              </div>

              {/* Results resize handle */}
              <div
                className="h-1.5 bg-surface-border hover:bg-accent cursor-row-resize flex-shrink-0 flex items-center justify-center transition-colors"
                onMouseDown={onResultsMouseDown}
              >
                <div className="w-8 h-0.5 bg-text-muted rounded" />
              </div>

              {/* Results panel */}
              <div style={{ height: queryPanelHeight }} className="flex-shrink-0">
                <ResultsGrid tab={activeTab} />
              </div>
            </>
          ) : (
            <Welcome />
          )}
        </div>
      </div>

      <StatusBar />
    </div>
  );
}

function TabButton({ tab }: { tab: import("@/lib/types").QueryTab }) {
  const { activeTabId, setActiveTab, closeTab } = useAppStore();
  const isActive = tab.id === activeTabId;

  return (
    <div
      className={cn(
        "flex items-center gap-1.5 px-3 h-7 text-xs rounded-t border-t cursor-pointer select-none group whitespace-nowrap",
        isActive
          ? "bg-surface text-text-primary border-accent border-t-2"
          : "bg-surface-raised text-text-secondary border-surface-border hover:bg-surface-overlay"
      )}
      onClick={() => setActiveTab(tab.id)}
    >
      <span>{tab.title}</span>
      {tab.running && <span className="text-accent animate-pulse">●</span>}
      <span
        className="opacity-0 group-hover:opacity-100 hover:text-red-400 ml-0.5 transition-opacity"
        onClick={(e) => { e.stopPropagation(); closeTab(tab.id); }}
      >
        ×
      </span>
    </div>
  );
}

function Welcome() {
  const { savedConnections, openTab } = useAppStore();

  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-6 text-text-secondary">
      <div className="text-center">
        <div className="text-4xl mb-3">🗄️</div>
        <h2 className="text-lg font-semibold text-text-primary mb-1">Catalyst DBench</h2>
        <p className="text-sm">Connect to a database to start querying</p>
      </div>
      {savedConnections.length > 0 && (
        <div className="flex flex-col gap-2 w-64">
          <p className="text-xs text-text-muted text-center">Recent connections</p>
          {savedConnections.slice(0, 5).map((conn) => (
            <button
              key={conn.id}
              className="flex items-center gap-2 px-3 py-2 bg-surface-raised hover:bg-surface-overlay rounded text-sm text-left transition-colors"
              onClick={() => openTab(conn.id, conn.name, conn.db_type)}
            >
              <span>{conn.db_type === "postgres" ? "🐘" : conn.db_type === "mongodb" ? "🍃" : "🗄️"}</span>
              <span className="font-medium">{conn.name}</span>
              <span className="text-text-muted text-xs ml-auto">{conn.host}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
