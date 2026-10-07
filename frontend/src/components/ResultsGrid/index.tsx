import { useMemo, useState, useCallback, useRef, useEffect } from "react";

// TanStack Table
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  flexRender,
  createColumnHelper,
  type SortingState,
  type ColumnDef,
} from "@tanstack/react-table";

// TanStack Virtual
import { useVirtualizer } from "@tanstack/react-virtual";

// Lucide icons
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  Copy,
  X,
  Edit3,
  Save,
  XCircle,
  Trash2,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";

// Components
import ConfirmEditModal from "./ConfirmEditModal";
import ContextMenu, { type ContextMenuEntry } from "../ContextMenu/index";
import ExplainPlan from "../ExplainPlan/index";
import MongoDocTree from "./MongoDocTree";

// Utils
import { cn, formatRowCount, formatDuration } from "@/lib/utils";
import { executeQuery } from "@/lib/commands";
import { useAppStore } from "@/stores/useAppStore";

// Types
import { displayValue } from "@/lib/types";
import type { QueryTab, Row, BatchStatementResult } from "@/lib/types";
import {
  quoteIdent,
  SQL_DB_TYPES,
  escapeCsvCell,
  exportCsv,
  exportJson,
  extractTableName,
  extractCollectionName,
  buildMongoUpdate,
  buildMongoDelete,
  sqlValue,
} from "./helpers";

interface Props {
  tab: QueryTab;
}

// ── Entry point ────────────────────────────────────────────────────────────────

export default function ResultsGrid({ tab }: Props) {
  if (tab.running) return <LoadingPanel label={tab.running_label} />;
  if (tab.error) return <ErrorPanel message={tab.error} />;

  if (tab.batch_results) {
    return <BatchResultsView tab={tab} results={tab.batch_results} />;
  }

  if (!tab.result) return <EmptyPanel />;

  if (tab.result.explain_plan) {
    return (
      <div className="h-full bg-surface">
        <ExplainPlan plan={tab.result.explain_plan} />
      </div>
    );
  }

  return <DataTable tab={tab} />;
}

// ── Batch results view ─────────────────────────────────────────────────────────

function BatchResultsView({
  tab,
  results,
}: {
  tab: QueryTab;
  results: BatchStatementResult[];
}) {
  const [activeIdx, setActiveIdx] = useState<"summary" | number>("summary");
  const { updateTab } = useAppStore();

  const stmtLabel = (r: BatchStatementResult, i: number) => {
    const keyword = r.sql.trimStart().split(/\s+/)[0].toUpperCase();
    return `${i + 1}. ${keyword}`;
  };

  const activeResult =
    activeIdx !== "summary" ? results[activeIdx] : null;

  const stmtTab = activeResult
    ? { ...tab, result: activeResult.result, sql: activeResult.sql, batch_results: undefined }
    : null;

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Tab bar */}
      <div className="flex-shrink-0 flex items-center gap-0.5 px-2 pt-1 bg-surface border-b border-border overflow-x-auto min-w-0">
        <button
          className={cn(
            "px-3 py-1 text-xs rounded-t border-b-2 transition-colors whitespace-nowrap",
            activeIdx === "summary"
              ? "border-accent text-text-primary font-medium"
              : "border-transparent text-text-muted hover:text-text-secondary",
          )}
          onClick={() => setActiveIdx("summary")}
        >
          Summary
        </button>
        {results.map((r, i) => (
          <button
            key={i}
            className={cn(
              "px-3 py-1 text-xs rounded-t border-b-2 transition-colors whitespace-nowrap font-mono",
              activeIdx === i
                ? "border-accent text-text-primary font-medium"
                : r.error
                ? "border-transparent text-red-400 hover:text-red-300"
                : "border-transparent text-text-muted hover:text-text-secondary",
            )}
            onClick={() => setActiveIdx(i)}
          >
            {stmtLabel(r, i)}
            {r.error && <span className="ml-1 text-red-400">✗</span>}
          </button>
        ))}
        <div className="ml-auto pl-2 flex-shrink-0">
          <button
            className="p-1 text-text-muted hover:text-text-secondary transition-colors"
            title="Dismiss batch results"
            onClick={() =>
              updateTab(tab.id, {
                batch_results: undefined,
                result: undefined,
              })
            }
          >
            <X size={12} />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        {activeIdx === "summary" && (
          <BatchSummary results={results} onSelect={setActiveIdx} />
        )}
        {activeIdx !== "summary" && activeResult && (
          <>
            {activeResult.error && (
              <ErrorPanel message={activeResult.error} />
            )}
            {!activeResult.error && stmtTab?.result && !stmtTab.result.explain_plan && (
              <DataTable tab={stmtTab} />
            )}
            {!activeResult.error && stmtTab?.result?.explain_plan && (
              <div className="h-full bg-surface">
                <ExplainPlan plan={stmtTab.result.explain_plan} />
              </div>
            )}
            {!activeResult.error && !stmtTab?.result && (
              <div className="h-full flex items-center justify-center text-text-muted text-sm">
                {results[activeIdx as number].sql
                  .trimStart()
                  .split(/\s+/)[0]
                  .toUpperCase()}{" "}
                executed —{" "}
                {results[activeIdx as number].result?.rows_affected ?? 0} rows affected
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

// ── DataTable ──────────────────────────────────────────────────────────────────

function DataTable({ tab }: Props) {
  const { result } = tab;
  if (!result) return null;

  const { schemas, updateTab, addToHistory } = useAppStore();

  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");
  const [viewMode, setViewMode] = useState<"table" | "tree">("table");

  const scrollRef = useRef<HTMLDivElement>(null);

  const [expandedCell, setExpandedCell] = useState<{
    header: string;
    value: string;
  } | null>(null);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);

  const [editMode, setEditMode] = useState(false);
  const [editingCell, setEditingCell] = useState<{
    rowIdx: number;
    colIdx: number;
  } | null>(null);
  const [pendingEdits, setPendingEdits] = useState<Record<string, string>>({});
  const [pendingDeletes, setPendingDeletes] = useState<Set<number>>(new Set());
  const [confirmSql, setConfirmSql] = useState<string | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);

  const [rowCtx, setRowCtx] = useState<{
    x: number;
    y: number;
    row: Row;
    rowIdx: number;
  } | null>(null);

  const isSqlDb = SQL_DB_TYPES.includes(tab.db_type);
  const isMongo = tab.db_type === "mongodb";
  const tableName = isMongo ? null : extractTableName(tab.sql);
  const collectionName = isMongo ? extractCollectionName(tab.sql) : null;
  const editTarget = tableName ?? collectionName;

  const mongoDb = useMemo(() => {
    if (!isMongo) return null;
    try {
      return (
        ((JSON.parse(tab.sql) as Record<string, unknown>).db as string) ?? null
      );
    } catch {
      return null;
    }
  }, [isMongo, tab.sql]);

  const schema = schemas[tab.connection_id];
  const pkColumn = useMemo(() => {
    if (
      !schema ||
      schema === "loading" ||
      typeof schema !== "object" ||
      "error" in schema
    )
      return null;
    if (isMongo) {
      const hasId = result.columns.some((c) => c.name === "_id");
      return hasId ? "_id" : (result.columns[0]?.name ?? null);
    }
    const tbl = schema.objects.find(
      (o) =>
        o.kind === "table" &&
        (o.name as string).toLowerCase() === tableName?.toLowerCase(),
    );
    const col = (
      tbl as { columns?: Array<{ name: string; is_primary_key: boolean }> }
    )?.columns?.find((c) => c.is_primary_key);
    return col?.name ?? null;
  }, [schema, tableName, isMongo, result.columns]);

  const pkColIdx = pkColumn
    ? result.columns.findIndex((c) => c.name === pkColumn)
    : -1;
  const headers = result.columns.map((c) => c.name);

  const columnHelper = createColumnHelper<Row>();

  const columns = useMemo<ColumnDef<Row, string>[]>(
    () =>
      result.columns.map(
        (col, i) =>
          columnHelper.accessor(
            (row) => displayValue(row.values[i] ?? { type: "null" }),
            {
              id: col.name,
              header: () => (
                <div className="flex flex-col gap-0.5">
                  <span className="font-semibold text-text-primary">
                    {col.name}
                  </span>
                  <span className="text-2xs text-text-muted font-normal">
                    {col.native_type}
                  </span>
                </div>
              ),
              cell: (info) => {
                const rowIdx = info.row.index;
                const editKey = `${rowIdx}-${i}`;
                const raw = result.rows[rowIdx]?.values[i];
                const isNull = !raw || raw.type === "null";
                const text = isNull ? "NULL" : displayValue(raw);
                const pendingValue = pendingEdits[editKey];
                const isEditing =
                  editMode &&
                  editingCell?.rowIdx === rowIdx &&
                  editingCell?.colIdx === i;
                const hasEdit = editKey in pendingEdits;

                if (isEditing) {
                  return (
                    <input
                      autoFocus
                      className="w-full bg-surface border border-accent rounded px-1 py-0 text-xs text-text-primary font-mono outline-none"
                      defaultValue={pendingValue ?? (isNull ? "" : text)}
                      onBlur={(e) => {
                        setPendingEdits((p) => ({
                          ...p,
                          [editKey]: e.target.value,
                        }));
                        setEditingCell(null);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter")
                          (e.target as HTMLInputElement).blur();
                        if (e.key === "Escape") setEditingCell(null);
                      }}
                    />
                  );
                }

                const isTruncated = text.length > 80;
                return (
                  <span
                    className={cn(
                      "font-mono block max-w-xs overflow-hidden text-ellipsis whitespace-nowrap",
                      isNull && "text-text-muted italic",
                      editMode &&
                        hasEdit &&
                        "bg-yellow-500/10 text-yellow-300 rounded px-0.5",
                      !editMode &&
                        isTruncated &&
                        "cursor-pointer hover:text-accent",
                    )}
                    title={
                      editMode
                        ? "Double-click to edit"
                        : isTruncated
                          ? "Click to expand"
                          : undefined
                    }
                    onClick={() => {
                      if (!editMode && isTruncated)
                        setExpandedCell({ header: col.name, value: text });
                    }}
                    onDoubleClick={() => {
                      if (editMode) setEditingCell({ rowIdx, colIdx: i });
                    }}
                  >
                    {hasEdit ? pendingValue : text}
                  </span>
                );
              },
              size: 160,
            },
          ) as ColumnDef<Row, string>,
      ),
    [result.columns, result.rows, editMode, editingCell, pendingEdits],
  );

  const table = useReactTable({
    data: result.rows,
    columns,
    state: { sorting, globalFilter },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
  });

  const allRows = table.getRowModel().rows;
  const totalFiltered = table.getFilteredRowModel().rows.length;

  const rowVirtualizer = useVirtualizer({
    count: allRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 29,
    overscan: 10,
  });

  const virtualItems = rowVirtualizer.getVirtualItems();
  const totalVirtualSize = rowVirtualizer.getTotalSize();
  const paddingTop =
    virtualItems.length > 0 ? (virtualItems[0]?.start ?? 0) : 0;
  const paddingBottom =
    virtualItems.length > 0
      ? totalVirtualSize - (virtualItems[virtualItems.length - 1]?.end ?? 0)
      : 0;

  // ── Row actions ────────────────────────────────────────────────────────────

  const copyRowJson = (row: Row) => {
    const obj = Object.fromEntries(
      headers.map((h, i) => [
        h,
        displayValue(row.values[i] ?? { type: "null" }),
      ]),
    );
    navigator.clipboard.writeText(JSON.stringify(obj, null, 2));
  };

  const copyRowCsv = (row: Row) => {
    const line = headers
      .map((_, i) =>
        escapeCsvCell(displayValue(row.values[i] ?? { type: "null" })),
      )
      .join(",");
    navigator.clipboard.writeText(line);
  };

  const generateInsert = (row: Row) => {
    if (!tableName) return;
    const cols = headers.join(", ");
    const vals = row.values
      .map((v) => sqlValue(v ?? { type: "null" }))
      .join(", ");
    navigator.clipboard.writeText(
      `INSERT INTO ${tableName} (${cols}) VALUES (${vals});`,
    );
  };

  const toggleDelete = (rowIdx: number) => {
    setPendingDeletes((prev) => {
      const next = new Set(prev);
      if (next.has(rowIdx)) next.delete(rowIdx);
      else next.add(rowIdx);
      return next;
    });
  };

  const buildRowContextItems = useCallback(
    (row: Row, rowIdx: number): ContextMenuEntry[] => {
      const items: ContextMenuEntry[] = [
        {
          label: "Copy row as JSON",
          icon: <Copy size={11} />,
          onClick: () => copyRowJson(row),
        },
        {
          label: "Copy row as CSV",
          icon: <Copy size={11} />,
          onClick: () => copyRowCsv(row),
        },
      ];
      if (isSqlDb && tableName) {
        items.push({
          label: "Copy as INSERT",
          icon: <Copy size={11} />,
          onClick: () => generateInsert(row),
        });
      }
      if ((isSqlDb && tableName) || (isMongo && collectionName)) {
        items.push({ separator: true });
        const pkVal =
          pkColIdx >= 0
            ? displayValue(row.values[pkColIdx] ?? { type: "null" })
            : null;
        items.push({
          label: pendingDeletes.has(rowIdx) ? "Undo delete" : "Delete row…",
          icon: <Trash2 size={11} />,
          danger: true,
          // Without a primary key there is no safe WHERE clause — never offer the delete.
          disabled: !pkVal || pkVal === "NULL",
          onClick: () => {
            if (pkVal && pkVal !== "NULL") {
              toggleDelete(rowIdx);
              if (!editMode) setEditMode(true);
            }
          },
        });
      }
      return items;
    },
    [
      isSqlDb,
      isMongo,
      tableName,
      collectionName,
      pkColIdx,
      pkColumn,
      pendingDeletes,
      editMode,
    ],
  );

  // ── Edit apply ─────────────────────────────────────────────────────────────

  const pendingEditCount = Object.keys(pendingEdits).length;
  const pendingDeleteCount = pendingDeletes.size;
  const totalPending = pendingEditCount + pendingDeleteCount;

  const buildCommands = useCallback((): string[] => {
    const cmds: string[] = [];
    for (const [key, newVal] of Object.entries(pendingEdits)) {
      const [rowIdxStr, colIdxStr] = key.split("-");
      const row = result.rows[parseInt(rowIdxStr)];
      const colName = headers[parseInt(colIdxStr)];
      if (isMongo && collectionName && pkColIdx >= 0) {
        const pkVal = displayValue(row.values[pkColIdx] ?? { type: "null" });
        cmds.push(
          buildMongoUpdate(
            collectionName,
            mongoDb,
            pkColumn!,
            pkVal,
            colName,
            newVal,
          ),
        );
      } else if (tableName && pkColumn && pkColIdx >= 0) {
        // Only ever emit a statement with a primary-key WHERE clause: a missing
        // WHERE would rewrite every row in the table.
        const where = `WHERE ${quoteIdent(tab.db_type, pkColumn)} = ${sqlValue(row.values[pkColIdx])}`;
        const val =
          newVal === "" || newVal === "NULL"
            ? "NULL"
            : `'${newVal.replace(/'/g, "''")}'`;
        cmds.push(`UPDATE ${tableName} SET ${quoteIdent(tab.db_type, colName)} = ${val} ${where};`);
      }
    }
    for (const rowIdx of pendingDeletes) {
      const row = result.rows[rowIdx];
      if (isMongo && collectionName && pkColIdx >= 0) {
        const pkVal = displayValue(row.values[pkColIdx] ?? { type: "null" });
        cmds.push(buildMongoDelete(collectionName, mongoDb, pkColumn!, pkVal));
      } else if (tableName && pkColumn && pkColIdx >= 0) {
        const where = `WHERE ${quoteIdent(tab.db_type, pkColumn)} = ${sqlValue(row.values[pkColIdx])}`;
        cmds.push(`DELETE FROM ${tableName} ${where};`);
      }
    }
    return cmds;
  }, [
    pendingEdits,
    pendingDeletes,
    tableName,
    collectionName,
    isMongo,
    mongoDb,
    headers,
    pkColIdx,
    pkColumn,
    result.rows,
    tab.db_type,
  ]);

  const handleConfirmExecute = async (rawText: string) => {
    setApplyError(null);
    const cmds = rawText
      .split("\n")
      .filter((s) => s.trim() && !s.trim().startsWith("--"));
    try {
      for (const cmd of cmds) {
        const t0 = Date.now();
        const res = await executeQuery({
          connection_id: tab.connection_id,
          sql: cmd.trim(),
        });
        addToHistory({
          id: crypto.randomUUID(),
          sql: cmd.trim(),
          conn_id: tab.connection_id,
          conn_name: tab.connection_name,
          duration_ms: res.duration_ms,
          row_count: res.rows_affected ?? 0,
          ts: t0,
        });
      }
      setPendingEdits({});
      setPendingDeletes(new Set());
      setConfirmSql(null);
      setEditMode(false);
      if (tab.sql.trim()) {
        updateTab(tab.id, { running: true, result: undefined });
        try {
          const refreshed = await executeQuery({
            connection_id: tab.connection_id,
            sql: tab.sql.trim(),
          });
          updateTab(tab.id, { result: refreshed, running: false });
        } catch {
          updateTab(tab.id, { running: false });
        }
      }
    } catch (e) {
      setApplyError(String(e));
    }
  };

  // ── MongoDB filter bar query handler ────────────────────────────────────────

  const handleFilterQuery = useCallback(
    (sql: string) => {
      updateTab(tab.id, {
        sql,
        running: true,
        result: undefined,
        error: undefined,
      });
      executeQuery({ connection_id: tab.connection_id, sql })
        .then((res) => updateTab(tab.id, { result: res, running: false }))
        .catch((err) =>
          updateTab(tab.id, { error: String(err), running: false }),
        );
    },
    [tab.id, tab.connection_id, updateTab],
  );

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="h-full flex flex-col bg-surface">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-surface-raised border-b border-surface-border flex-shrink-0 flex-wrap">
        <div className="flex items-center gap-1.5 text-xs text-green-400">
          <CheckCircle2 size={12} />
          <span className="font-medium">
            {formatRowCount(result.rows.length)}
          </span>
          {result.rows_affected != null && (
            <span className="text-text-muted">
              · {result.rows_affected} affected
            </span>
          )}
          <span className="text-text-muted">
            · {formatDuration(result.duration_ms)}
          </span>
          {globalFilter && totalFiltered !== result.rows.length && (
            <span className="text-yellow-400">· {totalFiltered} filtered</span>
          )}
        </div>

        <div className="flex-1" />

        {/* MongoDB view toggle */}
        {isMongo && (
          <div className="flex rounded border border-surface-border overflow-hidden text-xs">
            <button
              className={cn(
                "px-2 py-0.5 transition-colors",
                viewMode === "table"
                  ? "bg-surface-overlay text-text-primary"
                  : "text-text-muted hover:text-text-primary",
              )}
              onClick={() => setViewMode("table")}
            >
              Table
            </button>
            <button
              className={cn(
                "px-2 py-0.5 transition-colors border-l border-surface-border",
                viewMode === "tree"
                  ? "bg-surface-overlay text-text-primary"
                  : "text-text-muted hover:text-text-primary",
              )}
              onClick={() => setViewMode("tree")}
            >
              Tree
            </button>
          </div>
        )}

        {/* Edit mode controls */}
        {(isSqlDb || isMongo) && (
          <div className="flex items-center gap-1.5">
            {editMode && totalPending > 0 && (
              <button
                className={cn(
                  "flex items-center gap-1 px-2 py-0.5 text-xs text-white rounded transition-colors",
                  editTarget
                    ? "bg-accent hover:bg-accent-hover"
                    : "bg-text-muted cursor-not-allowed",
                )}
                onClick={() => setConfirmSql(buildCommands().join("\n"))}
                disabled={!editTarget}
              >
                <Save size={11} />
                Apply {totalPending} change{totalPending !== 1 ? "s" : ""}
              </button>
            )}
            {editMode && (
              <button
                className="flex items-center gap-1 px-2 py-0.5 text-xs text-text-secondary hover:text-red-400 border border-surface-border rounded transition-colors"
                onClick={() => {
                  setPendingEdits({});
                  setPendingDeletes(new Set());
                  setEditMode(false);
                }}
              >
                <XCircle size={11} /> Cancel
              </button>
            )}
            <button
              className={cn(
                "flex items-center gap-1 px-2 py-0.5 text-xs rounded border transition-colors",
                editMode
                  ? "border-accent text-accent bg-accent/10"
                  : "border-surface-border text-text-secondary hover:text-text-primary hover:border-text-muted",
              )}
              onClick={() => setEditMode((m) => !m)}
            >
              <Edit3 size={11} />
              {editMode ? "Editing" : "Edit"}
            </button>
          </div>
        )}

        {/* Filter */}
        <input
          className="bg-surface-overlay border border-surface-border rounded px-2 py-0.5 text-xs text-text-primary placeholder-text-muted outline-none focus:border-accent w-36"
          placeholder="Filter…"
          value={globalFilter}
          onChange={(e) => setGlobalFilter(e.target.value)}
        />

        {/* Export */}
        <div className="relative">
          <button
            className="flex items-center gap-1 px-2 py-0.5 text-xs text-text-secondary hover:text-text-primary border border-surface-border rounded transition-colors"
            onClick={() => setExportMenuOpen((o) => !o)}
          >
            <Download size={11} /> Export ▾
          </button>
          {exportMenuOpen && (
            <div className="absolute right-0 top-full mt-1 bg-surface-raised border border-surface-border rounded shadow-lg z-20 min-w-24">
              <button
                className="block w-full text-left px-3 py-1.5 text-xs text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
                onClick={() => {
                  void exportCsv(headers, result.rows);
                  setExportMenuOpen(false);
                }}
              >
                Export CSV
              </button>
              <button
                className="block w-full text-left px-3 py-1.5 text-xs text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
                onClick={() => {
                  void exportJson(headers, result.rows);
                  setExportMenuOpen(false);
                }}
              >
                Export JSON
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Edit mode hint bar */}
      {editMode && (
        <div className="px-3 py-1 bg-accent/10 border-b border-accent/30 text-2xs text-accent flex items-center gap-3">
          <span>
            Edit mode: double-click any cell · right-click a row for options
          </span>
          {!editTarget && (
            <span className="text-yellow-400 ml-auto">
              {isMongo
                ? "⚠ Collection not detected — use a find query"
                : "⚠ Table not detected — run a simple SELECT * FROM tablename"}
            </span>
          )}
          {editTarget && !pkColumn && (
            <span className="text-yellow-400 ml-auto">
              ⚠ No primary key detected for <code>{editTarget}</code>
            </span>
          )}
        </div>
      )}

      {/* MongoDB filter bar */}
      {isMongo && <MongoFilterBar tab={tab} onQuery={handleFilterQuery} />}

      {/* Tree view (MongoDB) */}
      {viewMode === "tree" && isMongo ? (
        <MongoDocTree result={result} />
      ) : (
        <>
          {/* Table */}
          <div
            ref={scrollRef}
            className="flex-1 overflow-auto"
            onClick={() => setExportMenuOpen(false)}
          >
            <table className="w-full text-xs border-collapse">
              <thead className="sticky top-0 z-10 bg-surface-overlay">
                {table.getHeaderGroups().map((hg) => (
                  <tr key={hg.id}>
                    <th className="w-12 px-2 py-1.5 text-right text-text-muted font-normal border-b border-surface-border select-none" />
                    {hg.headers.map((header) => (
                      <th
                        key={header.id}
                        className={cn(
                          "px-3 py-1.5 text-left border-b border-surface-border whitespace-nowrap",
                          header.column.getCanSort() &&
                            "cursor-pointer select-none hover:bg-surface-raised",
                        )}
                        style={{ width: header.getSize() }}
                        onClick={header.column.getToggleSortingHandler()}
                      >
                        <div className="flex items-center gap-1">
                          {flexRender(
                            header.column.columnDef.header,
                            header.getContext(),
                          )}
                          {header.column.getIsSorted() === "asc" && (
                            <span className="text-accent">↑</span>
                          )}
                          {header.column.getIsSorted() === "desc" && (
                            <span className="text-accent">↓</span>
                          )}
                        </div>
                      </th>
                    ))}
                  </tr>
                ))}
              </thead>
              <tbody>
                {paddingTop > 0 && (
                  <tr>
                    <td
                      colSpan={headers.length + 1}
                      style={{ height: paddingTop }}
                    />
                  </tr>
                )}
                {virtualItems.map((vr) => {
                  const row = allRows[vr.index];
                  if (!row) return null;
                  const realIdx = row.index;
                  const isDeleted = pendingDeletes.has(realIdx);
                  return (
                    <tr
                      key={row.id}
                      className={cn(
                        "border-b border-surface-border/50 group",
                        isDeleted
                          ? "bg-red-950/20 opacity-40"
                          : "hover:bg-surface-raised",
                      )}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setRowCtx({
                          x: e.clientX,
                          y: e.clientY,
                          row: result.rows[realIdx],
                          rowIdx: realIdx,
                        });
                      }}
                    >
                      <td className="px-2 py-1 text-right text-text-muted font-mono select-none">
                        <div className="flex items-center justify-end gap-1">
                          {editMode ? (
                            <button
                              className={cn(
                                "transition-colors",
                                isDeleted
                                  ? "text-red-400"
                                  : "text-text-muted hover:text-red-400",
                              )}
                              title={
                                isDeleted ? "Undo delete" : "Mark for deletion"
                              }
                              onClick={() => toggleDelete(realIdx)}
                            >
                              <Trash2 size={9} />
                            </button>
                          ) : (
                            <button
                              className="opacity-0 group-hover:opacity-100 text-text-muted hover:text-accent transition-opacity"
                              title="Copy row as JSON"
                              onClick={() => copyRowJson(result.rows[realIdx])}
                            >
                              <Copy size={9} />
                            </button>
                          )}
                          <span className={isDeleted ? "line-through" : ""}>
                            {vr.index + 1}
                          </span>
                        </div>
                      </td>
                      {row.getVisibleCells().map((cell) => (
                        <td
                          key={cell.id}
                          className="px-3 py-1 text-text-primary"
                        >
                          {flexRender(
                            cell.column.columnDef.cell,
                            cell.getContext(),
                          )}
                        </td>
                      ))}
                    </tr>
                  );
                })}
                {paddingBottom > 0 && (
                  <tr>
                    <td
                      colSpan={headers.length + 1}
                      style={{ height: paddingBottom }}
                    />
                  </tr>
                )}
              </tbody>
            </table>

            {result.rows.length === 0 && (
              <div className="flex items-center justify-center h-20 text-sm text-text-muted">
                Query returned no rows
              </div>
            )}
          </div>

          {/* Row count footer */}
          {totalFiltered > 0 && (
            <div className="flex items-center px-3 py-1 border-t border-surface-border bg-surface-raised flex-shrink-0 text-xs text-text-muted">
              <span>{totalFiltered.toLocaleString()} rows</span>
            </div>
          )}
        </>
      )}

      {/* Context menu */}
      {rowCtx && (
        <ContextMenu
          x={rowCtx.x}
          y={rowCtx.y}
          items={buildRowContextItems(rowCtx.row, rowCtx.rowIdx)}
          onClose={() => setRowCtx(null)}
        />
      )}

      {/* Cell expand modal */}
      {expandedCell && (
        <CellExpandModal
          header={expandedCell.header}
          value={expandedCell.value}
          onClose={() => setExpandedCell(null)}
        />
      )}

      {/* Confirm SQL execution modal */}
      {confirmSql !== null && (
        <ConfirmEditModal
          sql={confirmSql}
          error={applyError}
          onConfirm={handleConfirmExecute}
          onCancel={() => {
            setConfirmSql(null);
            setApplyError(null);
          }}
        />
      )}
    </div>
  );
}

// ── MongoDB filter bar ─────────────────────────────────────────────────────────

function MongoFilterBar({
  tab,
  onQuery,
}: {
  tab: QueryTab;
  onQuery: (sql: string) => void;
}) {
  const parsedSql = useMemo(() => {
    try { return JSON.parse(tab.sql) as Record<string, unknown>; }
    catch { return null; }
  }, [tab.sql]);

  const collection = useMemo(() => extractCollectionName(tab.sql), [tab.sql]);

  // tab.sql stores limit+1 (peek row); display limit is one less.
  const [filter, setFilter] = useState(() => {
    const f = parsedSql?.filter;
    return f && typeof f === "object" ? JSON.stringify(f) : "{}";
  });
  const [sort, setSort] = useState(() => {
    const s = parsedSql?.sort;
    return s && typeof s === "object" ? JSON.stringify(s) : "{}";
  });
  const [limit, setLimit] = useState(() =>
    typeof parsedSql?.limit === "number" ? Math.max(1, parsedSql.limit - 1) : 100,
  );
  const [page, setPage] = useState(() => {
    const displayLim = typeof parsedSql?.limit === "number" ? Math.max(1, parsedSql.limit - 1) : 100;
    const skip = typeof parsedSql?.skip === "number" ? parsedSql.skip : 0;
    return displayLim > 0 ? Math.floor(skip / displayLim) + 1 : 1;
  });

  // Build the query JSON. We request l+1 rows to peek if a next page exists.
  const buildQuery = useCallback((f: string, s: string, l: number, p: number): string => {
    const coll = collection ?? extractCollectionName(tab.sql) ?? "";
    const existing = (() => {
      try { return JSON.parse(tab.sql) as Record<string, unknown>; }
      catch { return {} as Record<string, unknown>; }
    })();
    const q: Record<string, unknown> = { find: coll };
    if (existing.db) q.db = existing.db;
    try {
      const fObj = JSON.parse(f) as Record<string, unknown>;
      if (Object.keys(fObj).length > 0) q.filter = fObj;
    } catch { /* invalid JSON */ }
    try {
      const sObj = JSON.parse(s) as Record<string, unknown>;
      if (Object.keys(sObj).length > 0) q.sort = sObj;
    } catch { /* invalid JSON */ }
    q.limit = l + 1; // fetch one extra to detect next page
    if (p > 1) q.skip = (p - 1) * l;
    return JSON.stringify(q);
  }, [collection, tab.sql]);

  // Derived from tab.sql: the stored limit is l+1, so display limit is l+1-1.
  const appliedDisplayLimit =
    typeof parsedSql?.limit === "number" ? parsedSql.limit - 1 : null;

  const rowCount = tab.result?.rows.length ?? 0;
  // More rows returned than the display limit → next page exists.
  const hasNextPage =
    appliedDisplayLimit !== null && rowCount > appliedDisplayLimit && !tab.running;
  const hasPrevPage = page > 1 && !tab.running;

  // Auto-navigate back if we land on an empty page (went past the last page).
  useEffect(() => {
    if (!tab.running && page > 1 && tab.result !== undefined && rowCount === 0) {
      const prevPage = page - 1;
      setPage(prevPage);
      onQuery(buildQuery(filter, sort, limit, prevPage));
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.result, tab.running]);

  if (!collection) return null;

  const applyPage = (newPage: number) => {
    setPage(newPage);
    onQuery(buildQuery(filter, sort, limit, newPage));
  };

  return (
    <div className="flex items-center gap-2 px-3 py-1 bg-surface border-b border-surface-border flex-shrink-0 text-xs flex-wrap">
      <span className="text-text-muted">Filter:</span>
      <input
        value={filter}
        onChange={(e) => {
          setFilter(e.target.value);
          setPage(1);
        }}
        className="bg-surface-overlay border border-surface-border rounded px-2 py-0.5 text-text-primary font-mono placeholder-text-muted outline-none focus:border-accent w-36"
        placeholder="{}"
        title="MongoDB filter (JSON)"
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            setPage(1);
            onQuery(buildQuery(filter, sort, limit, 1));
          }
        }}
      />
      <span className="text-text-muted">Sort:</span>
      <input
        value={sort}
        onChange={(e) => {
          setSort(e.target.value);
          setPage(1);
        }}
        className="bg-surface-overlay border border-surface-border rounded px-2 py-0.5 text-text-primary font-mono placeholder-text-muted outline-none focus:border-accent w-28"
        placeholder="{}"
        title='Sort (JSON, e.g. {"createdAt": -1})'
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            setPage(1);
            onQuery(buildQuery(filter, sort, limit, 1));
          }
        }}
      />
      <span className="text-text-muted">Limit:</span>
      <input
        type="number"
        value={limit}
        onChange={(e) => {
          setLimit(Math.max(1, Math.min(10000, Number(e.target.value))));
          setPage(1);
        }}
        className="bg-surface-overlay border border-surface-border rounded px-2 py-0.5 text-text-primary font-mono outline-none focus:border-accent w-16 [appearance:textfield]"
        min={1}
        max={10000}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            setPage(1);
            onQuery(buildQuery(filter, sort, limit, 1));
          }
        }}
      />
      <button
        className="px-2 py-0.5 bg-accent hover:bg-accent-hover text-white rounded transition-colors disabled:opacity-50"
        onClick={() => {
          setPage(1);
          onQuery(buildQuery(filter, sort, limit, 1));
        }}
        disabled={tab.running}
      >
        Apply
      </button>
      <button
        className="px-2 py-0.5 text-text-secondary hover:text-text-primary border border-surface-border rounded transition-colors disabled:opacity-50"
        onClick={() => {
          setFilter("{}");
          setSort("{}");
          setLimit(100);
          setPage(1);
          onQuery(buildQuery("{}", "{}", 100, 1));
        }}
        disabled={tab.running}
      >
        Reset
      </button>

      {/* Page navigation — shown whenever a limit-based query has been executed */}
      {appliedDisplayLimit !== null && (
        <div className="flex items-center gap-1 ml-auto">
          <button
            className="p-0.5 rounded text-text-muted hover:text-text-primary disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            onClick={() => applyPage(page - 1)}
            disabled={!hasPrevPage || tab.running}
            title="Previous page"
          >
            <ChevronLeft size={13} />
          </button>
          <span className="text-text-muted px-1">Page {page}</span>
          <button
            className="p-0.5 rounded text-text-muted hover:text-text-primary disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
            onClick={() => applyPage(page + 1)}
            disabled={!hasNextPage || tab.running}
            title="Next page"
          >
            <ChevronRight size={13} />
          </button>
        </div>
      )}
    </div>
  );
}

// ── Cell expand modal ─────────────────────────────────────────────────────────

function parseHexBytes(value: string): Uint8Array | null {
  const m = value.match(/^\\x([0-9a-fA-F]+)$/);
  if (!m) return null;
  const hex = m[1];
  if (hex.length % 2 !== 0) return null;
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

function isImageBytes(
  bytes: Uint8Array,
): "png" | "jpeg" | "gif" | "webp" | null {
  if (
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47
  )
    return "png";
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff)
    return "jpeg";
  if (bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) return "gif";
  if (
    bytes[0] === 0x52 &&
    bytes[1] === 0x49 &&
    bytes[2] === 0x46 &&
    bytes[4] === 0x57
  )
    return "webp";
  return null;
}

function HexDump({ bytes }: { bytes: Uint8Array }) {
  const rows: Array<{ offset: number; hex: string[]; ascii: string }> = [];
  for (let i = 0; i < Math.min(bytes.length, 512); i += 16) {
    const chunk = bytes.slice(i, i + 16);
    const hex = Array.from(chunk).map((b) => b.toString(16).padStart(2, "0"));
    const ascii = Array.from(chunk)
      .map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : "."))
      .join("");
    rows.push({ offset: i, hex, ascii });
  }
  return (
    <div className="font-mono text-2xs leading-5 select-text">
      <div className="text-text-muted mb-1 flex gap-4">
        <span className="w-12">Offset</span>
        <span className="flex-1">Hex</span>
        <span>ASCII</span>
      </div>
      {rows.map((row) => (
        <div
          key={row.offset}
          className="flex gap-4 hover:bg-surface-overlay rounded px-0.5"
        >
          <span className="w-12 text-text-muted">
            {row.offset.toString(16).padStart(4, "0")}
          </span>
          <span className="flex-1">
            {row.hex.slice(0, 8).join(" ")}
            {row.hex.length > 8 && <span className="mx-1.5" />}
            {row.hex.slice(8).join(" ")}
          </span>
          <span className="text-text-muted">{row.ascii}</span>
        </div>
      ))}
      {bytes.length > 512 && (
        <div className="text-text-muted mt-1">
          … {bytes.length - 512} more bytes
        </div>
      )}
    </div>
  );
}

function CellExpandModal({
  header,
  value,
  onClose,
}: {
  header: string;
  value: string;
  onClose: () => void;
}) {
  const bytes = parseHexBytes(value);
  const imageType = bytes ? isImageBytes(bytes) : null;
  const imageUrl =
    imageType && bytes
      ? URL.createObjectURL(
          new Blob([new Uint8Array(bytes)], { type: `image/${imageType}` }),
        )
      : null;

  const displayText = (() => {
    if (bytes) return value;
    try {
      return JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      return value;
    }
  })();

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="bg-surface-raised border border-surface-border rounded-lg w-[600px] max-h-[80vh] flex flex-col shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-surface-border">
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold text-text-primary">
              {header}
            </span>
            {bytes && (
              <span className="text-2xs text-text-muted bg-surface-overlay rounded px-1.5 py-0.5">
                {bytes.length} bytes
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <button
              className="text-xs text-text-muted hover:text-accent flex items-center gap-1"
              onClick={() => navigator.clipboard.writeText(value)}
            >
              <Copy size={11} /> Copy
            </button>
            <button
              className="text-text-muted hover:text-text-primary"
              onClick={onClose}
            >
              <X size={14} />
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-auto p-4">
          {imageUrl ? (
            <div className="space-y-3">
              <img
                src={imageUrl}
                alt={header}
                className="max-w-full max-h-64 rounded border border-surface-border object-contain"
                onLoad={() => URL.revokeObjectURL(imageUrl)}
              />
              <HexDump bytes={bytes!} />
            </div>
          ) : bytes ? (
            <HexDump bytes={bytes} />
          ) : (
            <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap break-all">
              {displayText}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Batch summary ──────────────────────────────────────────────────────────────

function BatchSummary({
  results,
  onSelect,
}: {
  results: BatchStatementResult[];
  onSelect?: (idx: number) => void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const { theme } = useAppStore();
  const errorBg = theme === "dark" ? "#3b1a1a" : "#fff0f0";
  const errorBorder = theme === "dark" ? "#7f1d1d" : "#fca5a5";

  return (
    <div className="flex-shrink-0 border-b border-border overflow-auto" style={{ maxHeight: 180 }}>
      <table className="w-full text-xs border-collapse">
        <thead>
          <tr className="bg-surface-raised text-text-muted sticky top-0">
            <th className="px-3 py-1.5 text-left font-medium w-8">#</th>
            <th className="px-3 py-1.5 text-left font-medium">Statement</th>
            <th className="px-3 py-1.5 text-right font-medium w-32">Result</th>
            <th className="px-3 py-1.5 text-right font-medium w-20">Time</th>
          </tr>
        </thead>
        <tbody>
          {results.map((r, i) => {
            const isError = !!r.error;
            const rowsAffected = r.result?.rows_affected;
            const rowCount = r.result?.rows.length ?? 0;
            const durationMs = r.result?.duration_ms;
            const resultLabel = isError
              ? r.error
              : rowsAffected !== undefined && rowsAffected !== null
              ? `${rowsAffected} row${rowsAffected === 1 ? "" : "s"} affected`
              : `${rowCount} row${rowCount === 1 ? "" : "s"}`;

            return (
              <tr
                key={i}
                className="border-t border-border cursor-pointer"
                style={
                  isError
                    ? { backgroundColor: errorBg }
                    : selected === i
                    ? { backgroundColor: "var(--color-accent-subtle)" }
                    : undefined
                }
                onClick={() => {
                  setSelected(selected === i ? null : i);
                  onSelect?.(i);
                }}
              >
                <td className="px-3 py-1.5 text-text-muted font-mono">{i + 1}</td>
                <td className="px-3 py-1.5 font-mono text-text-primary truncate max-w-0 w-full">
                  {r.sql.replace(/\s+/g, " ").slice(0, 120)}
                </td>
                <td
                  className="px-3 py-1.5 text-right font-mono truncate"
                  style={{ color: isError ? (theme === "dark" ? "#f87171" : "#dc2626") : "var(--color-text-secondary)" }}
                >
                  {isError ? "✗ " : "✓ "}
                  {isError ? "failed" : resultLabel}
                </td>
                <td className="px-3 py-1.5 text-right text-text-muted">
                  {durationMs !== undefined ? `${durationMs}ms` : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {selected !== null && results[selected]?.error && (
        <div
          className="px-3 py-2 font-mono text-xs whitespace-pre-wrap border-t"
          style={{ borderColor: errorBorder, color: theme === "dark" ? "#f87171" : "#dc2626", backgroundColor: errorBg }}
        >
          {results[selected].error}
        </div>
      )}
    </div>
  );
}

// ── State panels ───────────────────────────────────────────────────────────────

function LoadingPanel({ label }: { label?: string }) {
  return (
    <div className="h-full flex items-center justify-center gap-2 text-text-muted text-sm">
      <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin" />
      {label ?? "Executing query…"}
    </div>
  );
}

function ErrorPanel({ message }: { message: string }) {
  return (
    <div className="h-full flex items-start gap-3 p-4 overflow-auto">
      <AlertTriangle size={16} className="text-red-400 flex-shrink-0 mt-0.5" />
      <div>
        <p className="text-sm text-red-400 font-medium mb-1">Query failed</p>
        <pre className="text-xs text-text-secondary font-mono whitespace-pre-wrap">
          {message}
        </pre>
      </div>
    </div>
  );
}

function EmptyPanel() {
  return (
    <div className="h-full flex items-center justify-center text-text-muted text-sm">
      Run a query to see results
    </div>
  );
}
