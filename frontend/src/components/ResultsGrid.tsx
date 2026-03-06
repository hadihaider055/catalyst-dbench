import { useMemo } from "react";
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  flexRender,
  createColumnHelper,
  type SortingState,
} from "@tanstack/react-table";
import { useState } from "react";
import { AlertTriangle, CheckCircle2, Download } from "lucide-react";
import { cn, formatRowCount, formatDuration } from "@/lib/utils";
import { displayValue } from "@/lib/types";
import type { QueryTab, Row } from "@/lib/types";

interface Props { tab: QueryTab }

export default function ResultsGrid({ tab }: Props) {
  if (tab.running) return <Loading />;
  if (tab.error) return <ErrorPanel message={tab.error} />;
  if (!tab.result) return <Empty />;

  const { result } = tab;

  if (result.explain_plan) {
    return (
      <div className="h-full bg-surface overflow-auto p-4">
        <pre className="text-xs text-text-primary font-mono">{result.explain_plan}</pre>
      </div>
    );
  }

  return <DataTable tab={tab} />;
}

function DataTable({ tab }: Props) {
  const { result } = tab;
  if (!result) return null;

  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");

  const columnHelper = createColumnHelper<Row>();

  const columns = useMemo(
    () =>
      result.columns.map((col, i) =>
        columnHelper.accessor((row) => displayValue(row.values[i] ?? { type: "null" }), {
          id: col.name,
          header: () => (
            <div className="flex flex-col gap-0.5">
              <span className="font-semibold text-text-primary">{col.name}</span>
              <span className="text-2xs text-text-muted font-normal">{col.native_type}</span>
            </div>
          ),
          cell: (info) => {
            const raw = result.rows[info.row.index]?.values[i];
            const isNull = !raw || raw.type === "null";
            return (
              <span className={cn("font-mono", isNull && "text-text-muted italic")}>
                {isNull ? "NULL" : displayValue(raw)}
              </span>
            );
          },
          size: 160,
        })
      ),
    [result.columns, result.rows]
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

  return (
    <div className="h-full flex flex-col bg-surface">
      {/* Results toolbar */}
      <div className="flex items-center gap-3 px-3 py-1.5 bg-surface-raised border-b border-surface-border flex-shrink-0">
        <div className="flex items-center gap-1.5 text-xs text-green-400">
          <CheckCircle2 size={12} />
          <span className="font-medium">{formatRowCount(result.rows.length)}</span>
          {result.rows_affected != null && (
            <span className="text-text-muted">· {result.rows_affected} affected</span>
          )}
          <span className="text-text-muted">· {formatDuration(result.duration_ms)}</span>
        </div>

        <div className="flex-1" />

        <input
          className="bg-surface-overlay border border-surface-border rounded px-2 py-0.5 text-xs text-text-primary placeholder-text-muted outline-none focus:border-accent w-40"
          placeholder="Filter results…"
          value={globalFilter}
          onChange={(e) => setGlobalFilter(e.target.value)}
        />

        <button
          className="flex items-center gap-1 px-2 py-0.5 text-xs text-text-secondary hover:text-text-primary transition-colors"
          title="Export CSV"
        >
          <Download size={11} /> Export
        </button>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs border-collapse">
          <thead className="sticky top-0 z-10 bg-surface-overlay">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {/* Row number */}
                <th className="w-10 px-2 py-1.5 text-right text-text-muted font-normal border-b border-surface-border select-none">
                  #
                </th>
                {hg.headers.map((header) => (
                  <th
                    key={header.id}
                    className={cn(
                      "px-3 py-1.5 text-left border-b border-surface-border whitespace-nowrap",
                      header.column.getCanSort() && "cursor-pointer select-none hover:bg-surface-raised"
                    )}
                    style={{ width: header.getSize() }}
                    onClick={header.column.getToggleSortingHandler()}
                  >
                    <div className="flex items-center gap-1">
                      {flexRender(header.column.columnDef.header, header.getContext())}
                      {header.column.getIsSorted() === "asc" && " ↑"}
                      {header.column.getIsSorted() === "desc" && " ↓"}
                    </div>
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row, idx) => (
              <tr
                key={row.id}
                className="hover:bg-surface-raised border-b border-surface-border/50 group"
              >
                <td className="px-2 py-1 text-right text-text-muted font-mono select-none">
                  {idx + 1}
                </td>
                {row.getVisibleCells().map((cell) => (
                  <td
                    key={cell.id}
                    className="px-3 py-1 text-text-primary whitespace-nowrap overflow-hidden max-w-xs"
                    title={String(cell.getValue())}
                  >
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>

        {result.rows.length === 0 && (
          <div className="flex items-center justify-center h-20 text-sm text-text-muted">
            Query returned no rows
          </div>
        )}
      </div>
    </div>
  );
}

function Loading() {
  return (
    <div className="h-full flex items-center justify-center gap-2 text-text-muted text-sm">
      <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin" />
      Executing query…
    </div>
  );
}

function ErrorPanel({ message }: { message: string }) {
  return (
    <div className="h-full flex items-start gap-3 p-4">
      <AlertTriangle size={16} className="text-red-400 flex-shrink-0 mt-0.5" />
      <div>
        <p className="text-sm text-red-400 font-medium mb-1">Query failed</p>
        <pre className="text-xs text-text-secondary font-mono whitespace-pre-wrap">{message}</pre>
      </div>
    </div>
  );
}

function Empty() {
  return (
    <div className="h-full flex items-center justify-center text-text-muted text-sm">
      Run a query to see results
    </div>
  );
}
