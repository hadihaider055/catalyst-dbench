import { useMemo, useState, useCallback } from "react";

// TanStack Table
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  flexRender,
  createColumnHelper,
  type SortingState,
  type PaginationState,
  type ColumnDef,
} from "@tanstack/react-table";

// Lucide icons
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  Copy,
  ChevronLeft,
  ChevronRight,
  X,
  Edit3,
  Save,
  XCircle,
  Trash2,
} from "lucide-react";

// Components
import ConfirmEditModal from "./ConfirmEditModal";
import ContextMenu, { type ContextMenuEntry } from "../ContextMenu/index";

// Utils
import { cn, formatRowCount, formatDuration } from "@/lib/utils";
import { executeQuery } from "@/lib/commands";
import { useAppStore } from "@/stores/useAppStore";

// Types
import { displayValue } from "@/lib/types";
import type { QueryTab, Row } from "@/lib/types";
import {
  SQL_DB_TYPES,
  PAGE_SIZE_OPTIONS,
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
  if (tab.running) return <LoadingPanel />;
  if (tab.error) return <ErrorPanel message={tab.error} />;
  if (!tab.result) return <EmptyPanel />;

  if (tab.result.explain_plan) {
    return (
      <div className="h-full bg-surface overflow-auto p-4">
        <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap">
          {tab.result.explain_plan}
        </pre>
      </div>
    );
  }

  return <DataTable tab={tab} />;
}

// ── DataTable ──────────────────────────────────────────────────────────────────

function DataTable({ tab }: Props) {
  const { result } = tab;
  if (!result) return null;

  const { schemas, updateTab, addToHistory } = useAppStore();

  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 100,
  });
  const { pageIndex, pageSize } = pagination;

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
  // The effective "table" identifier for edit operations
  const editTarget = tableName ?? collectionName;

  // For MongoDB, extract the db override from the query JSON
  const mongoDb = useMemo(() => {
    if (!isMongo) return null;
    try { return (JSON.parse(tab.sql) as Record<string, unknown>).db as string ?? null; }
    catch { return null; }
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
      // MongoDB: use "_id" as the default PK — always present
      const hasId = result.columns.some((c) => c.name === "_id");
      return hasId ? "_id" : result.columns[0]?.name ?? null;
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
    state: { sorting, globalFilter, pagination },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    onPaginationChange: setPagination,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    autoResetPageIndex: false,
  });

  const visibleRows = table.getRowModel().rows;
  const totalFiltered = table.getFilteredRowModel().rows.length;
  const pageCount = table.getPageCount();

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
        const pkVal = pkColIdx >= 0 ? displayValue(row.values[pkColIdx] ?? { type: "null" }) : null;
        items.push({
          label: pendingDeletes.has(rowIdx) ? "Undo delete" : "Delete row…",
          icon: <Trash2 size={11} />,
          danger: true,
          onClick: () => {
            if (!pkVal || pkVal === "NULL") {
              if (isSqlDb && tableName) {
                setConfirmSql(`DELETE FROM ${tableName} -- WARNING: no PK detected;`);
              }
            } else {
              toggleDelete(rowIdx);
              if (!editMode) setEditMode(true);
            }
          },
        });
      }
      return items;
    },
    [isSqlDb, isMongo, tableName, collectionName, pkColIdx, pkColumn, pendingDeletes, editMode],
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
        cmds.push(buildMongoUpdate(collectionName, mongoDb, pkColumn!, pkVal, colName, newVal));
      } else if (tableName) {
        const pkVal = pkColIdx >= 0 ? sqlValue(row.values[pkColIdx]) : null;
        const where = pkVal ? `WHERE ${pkColumn} = ${pkVal}` : "-- WARNING: no PK found";
        const val = newVal === "" || newVal === "NULL" ? "NULL" : `'${newVal.replace(/'/g, "''")}'`;
        cmds.push(`UPDATE ${tableName} SET \`${colName}\` = ${val} ${where};`);
      }
    }
    for (const rowIdx of pendingDeletes) {
      const row = result.rows[rowIdx];
      if (isMongo && collectionName && pkColIdx >= 0) {
        const pkVal = displayValue(row.values[pkColIdx] ?? { type: "null" });
        cmds.push(buildMongoDelete(collectionName, mongoDb, pkColumn!, pkVal));
      } else if (tableName) {
        const pkVal = pkColIdx >= 0 ? sqlValue(row.values[pkColIdx]) : null;
        const where = pkVal ? `WHERE ${pkColumn} = ${pkVal}` : "-- WARNING: no PK found";
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
  ]);

  const handleConfirmExecute = async (rawText: string) => {
    setApplyError(null);
    // Each line is a separate command (SQL statement or MongoDB JSON command)
    const cmds = rawText.split("\n").filter((s) => s.trim() && !s.trim().startsWith("--"));
    try {
      for (const cmd of cmds) {
        const t0 = Date.now();
        const res = await executeQuery({ connection_id: tab.connection_id, sql: cmd.trim() });
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
          const refreshed = await executeQuery({ connection_id: tab.connection_id, sql: tab.sql.trim() });
          updateTab(tab.id, { result: refreshed, running: false });
        } catch {
          updateTab(tab.id, { running: false });
        }
      }
    } catch (e) {
      setApplyError(String(e));
    }
  };

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
          onChange={(e) => {
            setGlobalFilter(e.target.value);
            setPagination((p) => ({ ...p, pageIndex: 0 }));
          }}
        />

        {/* Page size */}
        <select
          className="bg-surface-overlay border border-surface-border rounded px-1.5 py-0.5 text-xs text-text-secondary outline-none cursor-pointer"
          value={pageSize}
          onChange={(e) =>
            setPagination({ pageIndex: 0, pageSize: Number(e.target.value) })
          }
        >
          {PAGE_SIZE_OPTIONS.map((n) => (
            <option key={n} value={n}>
              {n} rows
            </option>
          ))}
        </select>

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
                  exportCsv(headers, result.rows);
                  setExportMenuOpen(false);
                }}
              >
                Export CSV
              </button>
              <button
                className="block w-full text-left px-3 py-1.5 text-xs text-text-secondary hover:bg-surface-overlay hover:text-text-primary"
                onClick={() => {
                  exportJson(headers, result.rows);
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

      {/* Table */}
      <div
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
            {visibleRows.map((row, idx) => {
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
                        {pageIndex * pageSize + idx + 1}
                      </span>
                    </div>
                  </td>
                  {row.getVisibleCells().map((cell) => (
                    <td key={cell.id} className="px-3 py-1 text-text-primary">
                      {flexRender(
                        cell.column.columnDef.cell,
                        cell.getContext(),
                      )}
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>

        {result.rows.length === 0 && (
          <div className="flex items-center justify-center h-20 text-sm text-text-muted">
            Query returned no rows
          </div>
        )}
      </div>

      {/* Pagination */}
      {pageCount > 1 && (
        <div className="flex items-center gap-2 px-3 py-1 border-t border-surface-border bg-surface-raised flex-shrink-0 text-xs text-text-muted">
          <button
            className="p-0.5 hover:text-text-primary disabled:opacity-30"
            onClick={() => table.previousPage()}
            disabled={!table.getCanPreviousPage()}
          >
            <ChevronLeft size={13} />
          </button>
          <span>
            Page {pageIndex + 1} of {pageCount}
          </span>
          <button
            className="p-0.5 hover:text-text-primary disabled:opacity-30"
            onClick={() => table.nextPage()}
            disabled={!table.getCanNextPage()}
          >
            <ChevronRight size={13} />
          </button>
          <span className="ml-auto">{totalFiltered.toLocaleString()} rows</span>
        </div>
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
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
          onClick={() => setExpandedCell(null)}
        >
          <div
            className="bg-surface-raised border border-surface-border rounded-lg w-[600px] max-h-[80vh] flex flex-col shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-4 py-3 border-b border-surface-border">
              <span className="text-xs font-semibold text-text-primary">
                {expandedCell.header}
              </span>
              <div className="flex items-center gap-2">
                <button
                  className="text-xs text-text-muted hover:text-accent flex items-center gap-1"
                  onClick={() =>
                    navigator.clipboard.writeText(expandedCell.value)
                  }
                >
                  <Copy size={11} /> Copy
                </button>
                <button
                  className="text-text-muted hover:text-text-primary"
                  onClick={() => setExpandedCell(null)}
                >
                  <X size={14} />
                </button>
              </div>
            </div>
            <div className="flex-1 overflow-auto p-4">
              <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap break-all">
                {(() => {
                  try {
                    return JSON.stringify(
                      JSON.parse(expandedCell.value),
                      null,
                      2,
                    );
                  } catch {
                    return expandedCell.value;
                  }
                })()}
              </pre>
            </div>
          </div>
        </div>
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

// ── State panels ───────────────────────────────────────────────────────────────

function LoadingPanel() {
  return (
    <div className="h-full flex items-center justify-center gap-2 text-text-muted text-sm">
      <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin" />
      Executing query…
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
