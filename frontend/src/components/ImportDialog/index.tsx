import { useState, useRef } from "react";
import { X, Upload, FileText, AlertCircle, CheckCircle2 } from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import { executeBatch } from "@/lib/commands";
import { cn } from "@/lib/utils";

// ── CSV parser ────────────────────────────────────────────────────────────────

function parseCsv(text: string): { headers: string[]; rows: string[][] } {
  const lines = text.trim().split(/\r?\n/);
  if (lines.length < 2) return { headers: [], rows: [] };

  const parseRow = (line: string): string[] => {
    const cols: string[] = [];
    let cur = "";
    let inQ = false;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (ch === '"' && !inQ && cur === "") { inQ = true; continue; }
      if (ch === '"' && inQ) {
        if (line[i + 1] === '"') { cur += '"'; i++; }
        else inQ = false;
        continue;
      }
      if (ch === "," && !inQ) { cols.push(cur); cur = ""; continue; }
      cur += ch;
    }
    cols.push(cur);
    return cols;
  };

  const headers = parseRow(lines[0]);
  const rows = lines.slice(1).filter((l) => l.trim()).map(parseRow);
  return { headers, rows };
}

function buildInsertSql(table: string, headers: string[], row: string[]): string {
  const cols = headers.map((h) => `"${h}"`).join(", ");
  const vals = row
    .map((v) => {
      if (v === "" || v.toLowerCase() === "null") return "NULL";
      const n = Number(v);
      if (!isNaN(n) && v !== "") return v;
      return `'${v.replace(/'/g, "''")}'`;
    })
    .join(", ");
  return `INSERT INTO ${table} (${cols}) VALUES (${vals});`;
}

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  connectionId: string;
  onClose: () => void;
  onSuccess: (rowCount: number) => void;
}

const BATCH_SIZE = 50;

export default function ImportDialog({ connectionId, onClose, onSuccess }: Props) {
  const [table, setTable] = useState("");
  const [file, setFile] = useState<{ name: string; headers: string[]; rows: string[][] } | null>(null);
  const [_mode, setMode] = useState<"csv" | "json">("csv");
  const [status, setStatus] = useState<{ ok: boolean; msg: string } | null>(null);
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState(0);
  const abortRef = useRef(false);

  const pickFile = async () => {
    const path = await openFileDialog({
      multiple: false,
      filters: [{ name: "CSV / JSON", extensions: ["csv", "json", "jsonl"] }],
    });
    if (!path || typeof path !== "string") return;

    const ext = path.split(".").pop()?.toLowerCase() ?? "csv";
    const text = await readTextFile(path);
    const fileName = path.split("/").pop() ?? path;

    if (ext === "csv") {
      setMode("csv");
      const { headers, rows } = parseCsv(text);
      if (!headers.length) {
        setStatus({ ok: false, msg: "CSV appears empty or has no headers." });
        return;
      }
      setFile({ name: fileName, headers, rows });
      setStatus(null);
      if (!table) setTable(fileName.replace(/\.csv$/i, "").replace(/[^a-zA-Z0-9_]/g, "_"));
    } else {
      setMode("json");
      try {
        let records: Record<string, unknown>[];
        if (ext === "jsonl") {
          records = text
            .trim()
            .split("\n")
            .filter(Boolean)
            .map((l) => JSON.parse(l) as Record<string, unknown>);
        } else {
          const raw = JSON.parse(text) as unknown;
          records = Array.isArray(raw)
            ? (raw as Record<string, unknown>[])
            : [raw as Record<string, unknown>];
        }
        if (!records.length) {
          setStatus({ ok: false, msg: "JSON file is empty." });
          return;
        }
        const headers = Object.keys(records[0]);
        const rows = records.map((r) => headers.map((h) => String(r[h] ?? "")));
        setFile({ name: fileName, headers, rows });
        setStatus(null);
        if (!table) setTable(fileName.replace(/\.json(l)?$/i, "").replace(/[^a-zA-Z0-9_]/g, "_"));
      } catch {
        setStatus({ ok: false, msg: "Failed to parse JSON. Make sure it's an array of objects or JSONL." });
      }
    }
  };

  const handleImport = async () => {
    if (!file || !table.trim()) {
      setStatus({ ok: false, msg: "Choose a file and enter a target table name." });
      return;
    }
    setImporting(true);
    setProgress(0);
    abortRef.current = false;

    try {
      let imported = 0;
      for (let i = 0; i < file.rows.length; i += BATCH_SIZE) {
        if (abortRef.current) break;
        const batch = file.rows.slice(i, i + BATCH_SIZE);
        const sqls = batch.map((row) => buildInsertSql(table.trim(), file.headers, row));
        await executeBatch(connectionId, sqls);
        imported += batch.length;
        setProgress(Math.round((imported / file.rows.length) * 100));
      }
      setImporting(false);
      onSuccess(imported);
    } catch (e) {
      setImporting(false);
      setStatus({ ok: false, msg: String(e) });
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-surface-raised border border-surface-border rounded-lg w-[540px] shadow-2xl flex flex-col max-h-[80vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-surface-border flex-shrink-0">
          <h2 className="text-sm font-semibold text-text-primary">Import CSV / JSON</h2>
          <button className="text-text-muted hover:text-text-primary transition-colors" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          {/* File picker */}
          <button
            className="w-full flex items-center gap-3 px-4 py-6 rounded border-2 border-dashed border-surface-border hover:border-accent text-text-muted hover:text-text-secondary transition-colors"
            onClick={pickFile}
          >
            <Upload size={18} />
            <span className="text-sm">
              {file ? file.name : "Click to pick a CSV or JSON file…"}
            </span>
          </button>

          {/* Target table */}
          <div>
            <label className="block text-xs text-text-secondary mb-1.5">Target Table</label>
            <input
              type="text"
              value={table}
              onChange={(e) => setTable(e.target.value)}
              placeholder="table_name"
              className="w-full bg-surface border border-surface-border rounded px-3 py-1.5 text-xs text-text-primary placeholder-text-muted outline-none focus:border-accent transition-colors"
            />
          </div>

          {/* Preview */}
          {file && file.rows.length > 0 && (
            <div>
              <div className="flex items-center gap-1.5 mb-1.5">
                <FileText size={12} className="text-text-muted" />
                <span className="text-xs text-text-secondary">
                  {file.rows.length.toLocaleString()} rows · {file.headers.length} columns
                </span>
              </div>
              <div className="overflow-x-auto rounded border border-surface-border">
                <table className="text-2xs w-full">
                  <thead>
                    <tr className="bg-surface-overlay">
                      {file.headers.slice(0, 8).map((h) => (
                        <th key={h} className="px-2 py-1 text-left font-medium text-text-secondary border-r border-surface-border last:border-r-0">
                          {h}
                        </th>
                      ))}
                      {file.headers.length > 8 && (
                        <th className="px-2 py-1 text-text-muted">+{file.headers.length - 8} more</th>
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {file.rows.slice(0, 5).map((row, i) => (
                      <tr key={i} className="border-t border-surface-border">
                        {row.slice(0, 8).map((v, j) => (
                          <td key={j} className="px-2 py-1 text-text-secondary truncate max-w-[120px] border-r border-surface-border last:border-r-0">
                            {v || <span className="text-text-muted italic">NULL</span>}
                          </td>
                        ))}
                        {row.length > 8 && <td className="px-2 py-1 text-text-muted">…</td>}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {file.rows.length > 5 && (
                <p className="text-2xs text-text-muted mt-1">Showing 5 of {file.rows.length} rows</p>
              )}
            </div>
          )}

          {/* Progress */}
          {importing && (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between text-xs text-text-secondary">
                <span>Importing… {progress}%</span>
                <button
                  className="text-red-400 hover:text-red-300 text-2xs"
                  onClick={() => { abortRef.current = true; }}
                >
                  Cancel
                </button>
              </div>
              <div className="w-full h-1.5 bg-surface-border rounded-full overflow-hidden">
                <div
                  className="h-full bg-accent transition-all duration-150 rounded-full"
                  style={{ width: `${progress}%` }}
                />
              </div>
            </div>
          )}

          {/* Status message */}
          {status && (
            <div
              className={cn(
                "text-xs px-3 py-2 rounded border flex items-center gap-2",
                status.ok
                  ? "bg-green-950 border-green-800 text-green-300"
                  : "bg-red-950 border-red-800 text-red-300",
              )}
            >
              {status.ok ? <CheckCircle2 size={12} /> : <AlertCircle size={12} />}
              {status.msg}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 px-5 py-4 border-t border-surface-border flex-shrink-0">
          <button
            className="px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors"
            onClick={onClose}
            disabled={importing}
          >
            Cancel
          </button>
          <button
            className="flex items-center gap-1.5 px-4 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-xs font-medium transition-colors disabled:opacity-50"
            onClick={handleImport}
            disabled={!file || !table.trim() || importing}
          >
            <Upload size={12} />
            Import {file ? `${file.rows.length.toLocaleString()} rows` : ""}
          </button>
        </div>
      </div>
    </div>
  );
}
