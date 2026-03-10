import { useRef, useCallback, useEffect, useState } from "react";

// Monaco Editor
import MonacoEditor, { useMonaco } from "@monaco-editor/react";
import type { editor } from "monaco-editor";

// Lucide icons
import {
  Play,
  Zap,
  AlignLeft,
  Clock,
  Minus,
  Plus,
  GitBranch,
  Bookmark,
  FolderOpen,
  HardDriveDownload,
} from "lucide-react";

// Utils
import { useAppStore } from "@/stores/useAppStore";
import { executeQuery } from "@/lib/commands";
import { formatDuration } from "@/lib/utils";
import type { QueryTab } from "@/lib/types";
import { cn } from "@/lib/utils";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";

// SQL snippets
import { SQL_DB_TYPES, getSnippets, SQL_KEYWORDS } from "./snippets";

interface Props {
  tab: QueryTab;
}

export default function QueryEditor({ tab }: Props) {
  const {
    updateTab,
    addToHistory,
    saveQuery,
    editorFontSize,
    setEditorFontSize,
    theme,
    activeConnections,
    addToast,
  } = useAppStore();
  const [savePrompt, setSavePrompt] = useState<string | null>(null);
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monaco = useMonaco();

  const isConnected = activeConnections.some((c) => c.id === tab.connection_id);
  const monacoTheme = theme === "dark" ? "vs-dark" : "vs";

  const [txStatus, setTxStatus] = useState<"idle" | "active">("idle");
  const [txMsg, setTxMsg] = useState<string | null>(null);

  const language = tab.db_type === "mongodb" ? "json" : "sql";
  const isSqlDb = SQL_DB_TYPES.includes(tab.db_type);

  const run = useCallback(
    async (explain = false) => {
      const sql = editorRef.current?.getValue() ?? tab.sql;
      if (!sql.trim()) return;

      const { activeConnections: conns } = useAppStore.getState();
      if (!conns.some((c) => c.id === tab.connection_id)) {
        updateTab(tab.id, {
          error:
            "Not connected. Reconnect to this database before running queries.",
          running: false,
        });
        return;
      }

      const firstLine = sql
        .trim()
        .split("\n")[0]
        .replace(/\s+/g, " ")
        .slice(0, 30);
      updateTab(tab.id, {
        sql,
        running: true,
        error: undefined,
        result: undefined,
        title: firstLine || tab.title,
      });

      const startTs = Date.now();
      try {
        const result = await executeQuery({
          connection_id: tab.connection_id,
          sql: sql.trim(),
          explain,
        });
        updateTab(tab.id, { result, running: false });
        addToHistory({
          id: crypto.randomUUID(),
          sql: sql.trim(),
          conn_id: tab.connection_id,
          conn_name: tab.connection_name,
          duration_ms: result.duration_ms,
          row_count: result.rows.length,
          ts: startTs,
        });
      } catch (err) {
        updateTab(tab.id, { error: String(err), running: false });
        addToHistory({
          id: crypto.randomUUID(),
          sql: sql.trim(),
          conn_id: tab.connection_id,
          conn_name: tab.connection_name,
          duration_ms: Date.now() - startTs,
          row_count: 0,
          ts: startTs,
          error: String(err),
        });
      }
    },
    [
      tab.id,
      tab.connection_id,
      tab.connection_name,
      tab.sql,
      tab.title,
      updateTab,
      addToHistory,
    ],
  );

  const runTx = useCallback(
    async (cmd: string) => {
      setTxMsg(null);
      try {
        await executeQuery({ connection_id: tab.connection_id, sql: cmd });
        if (cmd === "BEGIN" || cmd === "START TRANSACTION") {
          setTxStatus("active");
          setTxMsg("Transaction started");
        } else {
          setTxStatus("idle");
          setTxMsg(cmd === "COMMIT" ? "Committed" : "Rolled back");
        }
        setTimeout(() => setTxMsg(null), 2500);
      } catch (err) {
        setTxMsg(`Error: ${String(err).slice(0, 60)}`);
        setTimeout(() => setTxMsg(null), 4000);
      }
    },
    [tab.connection_id],
  );

  const format = useCallback(() => {
    editorRef.current?.getAction("editor.action.formatDocument")?.run();
  }, []);

  const openFile = useCallback(async () => {
    try {
      const filters = tab.db_type === "mongodb"
        ? [{ name: "JSON / Text", extensions: ["json", "txt"] }]
        : [{ name: "SQL / Text", extensions: ["sql", "txt", "cql"] }];
      const path = await open({ multiple: false, filters });
      if (!path || typeof path !== "string") return;
      const content = await readTextFile(path);
      const fileName = path.split("/").pop()?.replace(/\.[^.]+$/, "") ?? tab.title;
      updateTab(tab.id, { sql: content, title: fileName });
      editorRef.current?.setValue(content);
      addToast(`Opened ${path.split("/").pop() ?? path}`, "info");
    } catch (e) {
      addToast(`Open file failed: ${String(e)}`, "error");
    }
  }, [tab.id, tab.title, updateTab, addToast]);

  const saveFile = useCallback(async () => {
    try {
      const content = editorRef.current?.getValue() ?? tab.sql;
      const ext = tab.db_type === "mongodb" ? "json" : "sql";
      const path = await save({
        defaultPath: `${tab.title}.${ext}`,
        filters: [{ name: ext === "json" ? "JSON Files" : "SQL Files", extensions: [ext] }],
      });
      if (!path) return;
      await writeTextFile(path, content);
      addToast(`Saved to ${path.split("/").pop() ?? path}`);
    } catch (e) {
      addToast(`Save failed: ${String(e)}`, "error");
    }
  }, [tab.sql, tab.db_type, tab.title, addToast]);

  // ── Contextual schema-aware intellisense ────────────────────────────────────
  useEffect(() => {
    if (!monaco) return;
    const lang = language === "sql" ? "sql" : "json";

    const provider = monaco.languages.registerCompletionItemProvider(lang, {
      triggerCharacters: [" ", ".", "\n", "(", ","],
      provideCompletionItems: (model, position) => {
        const schema = useAppStore.getState().schemas[tab.connection_id];
        if (!schema || schema === "loading" || "error" in schema) {
          return { suggestions: [] };
        }

        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        // Text before cursor for context analysis
        const textBefore = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });

        const objects = schema.objects;
        type ColInfo = {
          name: string;
          native_type: string;
          is_primary_key?: boolean;
        };

        // ── Context: `tablename.` → columns of that specific table ──────────
        const dotMatch = textBefore.match(/(\w+)\.(\w*)$/);
        if (dotMatch) {
          const tblName = dotMatch[1];
          const tbl = objects.find(
            (o) => (o.name as string).toLowerCase() === tblName.toLowerCase(),
          );
          const cols: ColInfo[] =
            (tbl as { columns?: ColInfo[] })?.columns ?? [];
          return {
            suggestions: cols.map((col) => ({
              label: col.name,
              kind: col.is_primary_key
                ? monaco.languages.CompletionItemKind.EnumMember
                : monaco.languages.CompletionItemKind.Field,
              insertText: col.name,
              detail: `${col.native_type}${col.is_primary_key ? " · PK" : ""}`,
              sortText: "0" + col.name,
              range,
            })),
          };
        }

        // Build reusable completion sets
        const tableCompletions = objects.map((obj) => ({
          label: obj.name as string,
          kind: monaco.languages.CompletionItemKind.Class,
          insertText: obj.name as string,
          detail: obj.kind,
          documentation: `${obj.kind}: ${obj.name as string}`,
          sortText: "1" + (obj.name as string),
          range,
        }));

        const columnCompletions: import("monaco-editor").languages.CompletionItem[] =
          [];
        for (const obj of objects) {
          const cols: ColInfo[] =
            (obj as { columns?: ColInfo[] })?.columns ?? [];
          for (const col of cols) {
            columnCompletions.push({
              label: col.name,
              kind: col.is_primary_key
                ? monaco.languages.CompletionItemKind.EnumMember
                : monaco.languages.CompletionItemKind.Field,
              insertText: col.name,
              detail: `${obj.name as string}.${col.name} · ${col.native_type}${col.is_primary_key ? " · PK" : ""}`,
              sortText: "2" + col.name,
              range,
            });
          }
        }

        const keywordCompletions = SQL_KEYWORDS.map((kw) => ({
          label: kw,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: kw,
          sortText: "3" + kw,
          range,
        }));

        // ── Context: after FROM / JOIN / INTO / UPDATE / TABLE / TRUNCATE → tables ──
        if (
          /\b(FROM|JOIN|INTO|UPDATE|TABLE|TRUNCATE)\s+\w*$/i.test(textBefore)
        ) {
          return { suggestions: tableCompletions };
        }

        // ── Context: inside SELECT clause (before FROM) → columns + * ────────
        // Match everything after the last SELECT that doesn't contain FROM/WHERE/JOIN
        const afterSelect = textBefore.match(
          /\bSELECT\b((?:(?!\bFROM\b|\bWHERE\b|\bJOIN\b).)*?)$/is,
        );
        if (afterSelect) {
          return {
            suggestions: [
              {
                label: "*",
                kind: monaco.languages.CompletionItemKind.Keyword,
                insertText: "*",
                detail: "All columns",
                sortText: "0*",
                range,
              },
              ...columnCompletions,
              ...tableCompletions,
            ],
          };
        }

        // ── Context: after WHERE / SET / HAVING / ON / AND / OR / BY → columns ──
        if (
          /\b(WHERE|SET|HAVING|AND|OR|ON|BY|RETURNING)\s+\w*$/i.test(textBefore)
        ) {
          return { suggestions: [...columnCompletions, ...keywordCompletions] };
        }

        // ── Default: everything ───────────────────────────────────────────────
        return {
          suggestions: [
            ...keywordCompletions,
            ...tableCompletions,
            ...columnCompletions,
          ],
        };
      },
    });

    return () => provider.dispose();
  }, [monaco, tab.connection_id, language]);

  // ── Keyboard shortcuts ──────────────────────────────────────────────────────
  useEffect(() => {
    if (!monaco || !editorRef.current) return;
    const ed = editorRef.current;

    const runAction = ed.addAction({
      id: "catalyst.runQuery",
      label: "Run Query",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      run: () => run(false),
    });

    const explainAction = ed.addAction({
      id: "catalyst.explainQuery",
      label: "Explain Query",
      keybindings: [
        monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.Enter,
      ],
      run: () => run(true),
    });

    const formatAction = ed.addAction({
      id: "catalyst.formatQuery",
      label: "Format Query",
      keybindings: [
        monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyF,
      ],
      run: () => format(),
    });

    return () => {
      runAction.dispose();
      explainAction.dispose();
      formatAction.dispose();
    };
  }, [monaco, run, format]);

  useEffect(() => {
    editorRef.current?.updateOptions({ fontSize: editorFontSize });
  }, [editorFontSize]);

  useEffect(() => {
    if (monaco) monaco.editor.setTheme(monacoTheme);
  }, [monaco, monacoTheme]);

  const beginCmd = tab.db_type === "mysql" ? "START TRANSACTION" : "BEGIN";

  return (
    <div className="flex flex-col h-full bg-surface">
      {/* Main toolbar */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-surface-raised border-b border-surface-border flex-shrink-0">
        {!isConnected && (
          <span className="text-2xs text-red-400 px-2 py-0.5 bg-red-950/30 border border-red-800/50 rounded">
            Disconnected
          </span>
        )}

        <button
          className="flex items-center gap-1.5 px-3 py-1 bg-accent hover:bg-accent-hover text-white rounded text-xs font-medium transition-colors disabled:opacity-50"
          onClick={() => run(false)}
          disabled={tab.running || !isConnected}
          title={isConnected ? "Run query (Ctrl+Enter)" : "Not connected"}
        >
          <Play size={11} />
          {tab.running ? "Running…" : "Run"}
        </button>

        <button
          className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors disabled:opacity-50"
          onClick={() => run(true)}
          disabled={tab.running || !isConnected}
          title="Explain query (Ctrl+Shift+Enter)"
        >
          <Zap size={11} />
          Explain
        </button>

        <button
          className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors"
          onClick={format}
          title="Format SQL (Shift+Alt+F)"
        >
          <AlignLeft size={11} />
          Format
        </button>

        {savePrompt !== null ? (
          <div className="flex items-center gap-1">
            <input
              autoFocus
              value={savePrompt}
              onChange={(e) => setSavePrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && savePrompt.trim()) {
                  saveQuery(savePrompt.trim(), editorRef.current?.getValue() ?? tab.sql, tab.db_type);
                  addToast(`Query "${savePrompt.trim()}" saved`);
                  setSavePrompt(null);
                } else if (e.key === "Escape") {
                  setSavePrompt(null);
                }
              }}
              placeholder="Query name…"
              className="px-2 py-0.5 text-2xs bg-surface border border-accent rounded outline-none text-text-primary w-28"
            />
            <button
              className="text-2xs px-2 py-0.5 bg-accent text-white rounded"
              onClick={() => {
                if (savePrompt.trim()) {
                  saveQuery(savePrompt.trim(), editorRef.current?.getValue() ?? tab.sql, tab.db_type);
                  addToast(`Query "${savePrompt.trim()}" saved`);
                  setSavePrompt(null);
                }
              }}
            >
              Save
            </button>
            <button
              className="text-2xs px-1 py-0.5 text-text-muted hover:text-text-primary"
              onClick={() => setSavePrompt(null)}
            >
              ✕
            </button>
          </div>
        ) : (
          <button
            className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors"
            onClick={() => setSavePrompt(tab.title)}
            title="Save query"
          >
            <Bookmark size={11} />
            Save
          </button>
        )}

        <div className="w-px h-4 bg-surface-border mx-0.5" />

        <button
          className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors"
          onClick={openFile}
          title="Open SQL file"
        >
          <FolderOpen size={11} />
          Open
        </button>

        <button
          className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-overlay hover:bg-surface-border text-text-secondary rounded text-xs transition-colors"
          onClick={saveFile}
          title="Save SQL to file"
        >
          <HardDriveDownload size={11} />
          Save
        </button>

        <div className="flex-1" />

        {/* Font size controls */}
        <div className="flex items-center gap-0.5">
          <button
            className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
            onClick={() => setEditorFontSize(editorFontSize - 1)}
            title="Decrease font size"
          >
            <Minus size={10} />
          </button>
          <span className="text-2xs text-text-muted w-6 text-center">
            {editorFontSize}
          </span>
          <button
            className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
            onClick={() => setEditorFontSize(editorFontSize + 1)}
            title="Increase font size"
          >
            <Plus size={10} />
          </button>
        </div>

        {tab.result && (
          <div className="flex items-center gap-1 text-xs text-text-muted">
            <Clock size={10} />
            {formatDuration(tab.result.duration_ms)}
          </div>
        )}

        <span className="text-2xs text-text-muted truncate max-w-32">
          {tab.connection_name}
        </span>
      </div>

      {/* Transaction toolbar — SQL DBs only */}
      {isSqlDb && (
        <div className="flex items-center gap-1.5 px-3 py-1 bg-surface border-b border-surface-border flex-shrink-0">
          <GitBranch size={10} className="text-text-muted flex-shrink-0" />
          <span className="text-2xs text-text-muted mr-1">Transaction:</span>
          <button
            className={cn(
              "px-2 py-0.5 text-2xs rounded border transition-colors",
              txStatus === "active"
                ? "border-yellow-600 text-yellow-400 bg-yellow-950/30 cursor-not-allowed opacity-50"
                : "border-surface-border text-text-secondary hover:border-green-600 hover:text-green-400 hover:bg-green-950/20",
            )}
            onClick={() => txStatus === "idle" && runTx(beginCmd)}
            disabled={txStatus === "active"}
            title="Begin transaction"
          >
            BEGIN
          </button>
          <button
            className={cn(
              "px-2 py-0.5 text-2xs rounded border transition-colors",
              txStatus === "idle"
                ? "border-surface-border text-text-muted cursor-not-allowed opacity-40"
                : "border-surface-border text-text-secondary hover:border-accent hover:text-accent hover:bg-accent/10",
            )}
            onClick={() => txStatus === "active" && runTx("COMMIT")}
            disabled={txStatus === "idle"}
            title="Commit transaction"
          >
            COMMIT
          </button>
          <button
            className={cn(
              "px-2 py-0.5 text-2xs rounded border transition-colors",
              txStatus === "idle"
                ? "border-surface-border text-text-muted cursor-not-allowed opacity-40"
                : "border-surface-border text-text-secondary hover:border-red-600 hover:text-red-400 hover:bg-red-950/20",
            )}
            onClick={() => txStatus === "active" && runTx("ROLLBACK")}
            disabled={txStatus === "idle"}
            title="Rollback transaction"
          >
            ROLLBACK
          </button>
          {txStatus === "active" && (
            <span className="text-2xs text-yellow-400 ml-1 flex items-center gap-1">
              <span className="w-1.5 h-1.5 rounded-full bg-yellow-400 animate-pulse" />
              In transaction
            </span>
          )}
          {txMsg && (
            <span className="text-2xs text-text-muted ml-1">{txMsg}</span>
          )}
        </div>
      )}

      {/* Quick snippet bar */}
      <div className="flex items-center gap-1 px-3 py-1 bg-surface border-b border-surface-border flex-shrink-0 overflow-x-auto">
        {getSnippets(tab.db_type).map((s) => (
          <button
            key={s.label}
            className="px-2 py-0.5 text-2xs text-text-muted hover:text-text-primary bg-surface-raised hover:bg-surface-overlay rounded border border-surface-border whitespace-nowrap transition-colors"
            onClick={() => {
              const ed = editorRef.current;
              if (!ed) return;
              const sel = ed.getSelection();
              const range = sel ?? {
                startLineNumber: 1,
                startColumn: 1,
                endLineNumber: 1,
                endColumn: 1,
              };
              ed.executeEdits("snippet", [{ range, text: s.sql }]);
              ed.focus();
            }}
            title={s.sql}
          >
            {s.label}
          </button>
        ))}
      </div>

      {/* Monaco Editor */}
      <div className="flex-1 min-h-0">
        <MonacoEditor
          height="100%"
          language={language}
          value={tab.sql}
          theme={monacoTheme}
          onMount={(ed) => {
            editorRef.current = ed;
            ed.focus();
          }}
          onChange={(value) => updateTab(tab.id, { sql: value ?? "" })}
          options={{
            fontSize: editorFontSize,
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
            suggest: {
              showKeywords: true,
              showClasses: true,
              showFields: true,
            },
            quickSuggestions: { other: true, comments: false, strings: true },
            suggestOnTriggerCharacters: true,
            padding: { top: 12, bottom: 12 },
          }}
        />
      </div>
    </div>
  );
}
