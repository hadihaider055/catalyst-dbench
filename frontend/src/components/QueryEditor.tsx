import { useRef, useCallback } from "react";
import MonacoEditor from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { Play, Zap, AlignLeft, Clock } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { executeQuery } from "@/lib/commands";
import { formatDuration } from "@/lib/utils";
import type { QueryTab } from "@/lib/types";

interface Props { tab: QueryTab }

export default function QueryEditor({ tab }: Props) {
  const { updateTab } = useAppStore();
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);

  const language = tab.db_type === "mongodb" ? "javascript" : "sql";

  const run = useCallback(async (explain = false) => {
    const sql = editorRef.current?.getValue() ?? tab.sql;
    if (!sql.trim()) return;

    updateTab(tab.id, { sql, running: true, error: undefined, result: undefined });

    try {
      const result = await executeQuery({
        connection_id: tab.connection_id,
        sql: sql.trim(),
        explain,
      });
      updateTab(tab.id, { result, running: false });
    } catch (err) {
      updateTab(tab.id, { error: String(err), running: false });
    }
  }, [tab.id, tab.connection_id, tab.sql, updateTab]);

  const format = useCallback(() => {
    // Basic formatting — real implementation would use sql-formatter
    const model = editorRef.current?.getModel();
    if (!model) return;
    editorRef.current?.getAction("editor.action.formatDocument")?.run();
  }, []);

  return (
    <div className="flex flex-col h-full bg-surface">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-surface-raised border-b border-surface-border flex-shrink-0">
        <button
          className="flex items-center gap-1.5 px-3 py-1 bg-accent hover:bg-accent-hover text-white rounded text-xs font-medium transition-colors disabled:opacity-50"
          onClick={() => run(false)}
          disabled={tab.running}
          title="Run query (Ctrl+Enter)"
        >
          <Play size={11} />
          {tab.running ? "Running…" : "Run"}
        </button>

        <button
          className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors"
          onClick={() => run(true)}
          disabled={tab.running}
          title="Explain query"
        >
          <Zap size={11} />
          Explain
        </button>

        <button
          className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors"
          onClick={format}
          title="Format SQL"
        >
          <AlignLeft size={11} />
          Format
        </button>

        <div className="flex-1" />

        {tab.result && (
          <div className="flex items-center gap-1 text-xs text-text-muted">
            <Clock size={10} />
            {formatDuration(tab.result.duration_ms)}
          </div>
        )}

        <span className="text-2xs text-text-muted">
          {tab.connection_name}
        </span>
      </div>

      {/* Monaco Editor */}
      <div className="flex-1 min-h-0">
        <MonacoEditor
          height="100%"
          language={language}
          value={tab.sql}
          theme="vs-dark"
          onMount={(ed) => { editorRef.current = ed; }}
          onChange={(value) => updateTab(tab.id, { sql: value ?? "" })}
          options={{
            fontSize: 13,
            fontFamily: "'JetBrains Mono', 'Fira Code', Menlo, monospace",
            fontLigatures: true,
            lineHeight: 20,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            wordWrap: "on",
            automaticLayout: true,
            tabSize: 2,
            insertSpaces: true,
            renderLineHighlight: "line",
            suggest: { showKeywords: true },
            quickSuggestions: { strings: true },
            padding: { top: 12, bottom: 12 },
          }}
        />
      </div>
    </div>
  );
}
