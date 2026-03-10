import type { Row } from "@/lib/types";
import { displayValue } from "@/lib/types";
import { useAppStore } from "@/stores/useAppStore";

export const SQL_DB_TYPES = [
  "postgres", "mysql", "sqlite", "cockroachdb", "mssql", "oracle", "clickhouse",
];

export const PAGE_SIZE_OPTIONS = [50, 100, 500, 1000];

// ── Export helpers ─────────────────────────────────────────────────────────────

export function escapeCsvCell(value: string): string {
  if (value.includes(",") || value.includes('"') || value.includes("\n"))
    return `"${value.replace(/"/g, '""')}"`;
  return value;
}

export async function exportCsv(headers: string[], rows: Row[]) {
  const lines = [
    headers.map(escapeCsvCell).join(","),
    ...rows.map((r) => r.values.map((v) => escapeCsvCell(displayValue(v ?? { type: "null" }))).join(",")),
  ];
  await saveToDisk(lines.join("\n"), "export", "csv", `Exported ${rows.length} rows as CSV`);
}

export async function exportJson(headers: string[], rows: Row[]) {
  const data = rows.map((r) =>
    Object.fromEntries(headers.map((h, i) => [h, displayValue(r.values[i] ?? { type: "null" })]))
  );
  await saveToDisk(JSON.stringify(data, null, 2), "export", "json", `Exported ${rows.length} rows as JSON`);
}

/** Save text content to disk via Tauri save dialog. Works on all platforms (macOS WKWebView-safe). */
export async function saveToDisk(
  content: string,
  baseName: string,
  ext: string,
  successMessage?: string,
): Promise<void> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const { writeTextFile } = await import("@tauri-apps/plugin-fs");
  const ts = new Date().toISOString().slice(0, 19).replace(/:/g, "-");
  try {
    const path = await save({
      defaultPath: `${baseName}-${ts}.${ext}`,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (!path) return; // user cancelled
    await writeTextFile(path, content);
    const fileName = path.split("/").pop() ?? path.split("\\").pop() ?? path;
    useAppStore.getState().addToast(successMessage ?? `Saved to ${fileName}`);
  } catch (e) {
    useAppStore.getState().addToast(`Save failed: ${String(e)}`, "error");
  }
}

/** @deprecated Use saveToDisk instead — blob URL downloads don't work in macOS WKWebView */
export function triggerDownload(_blob: Blob, _ext: string) {
  // No-op: replaced by saveToDisk. Kept for API compatibility.
}

// ── SQL helpers ────────────────────────────────────────────────────────────────

/**
 * Extract just the table name from SQL, handling:
 *   FROM table
 *   FROM db.table  (skips the db prefix)
 *   FROM `db`.`table`
 *   FROM "schema"."table"
 */
export function extractTableName(sql: string): string | null {
  const m = sql.match(/\bFROM\b\s+(?:\w+\.)?`?"?(\w+)`?"?/i);
  return m?.[1] ?? null;
}

// ── MongoDB helpers ─────────────────────────────────────────────────────────────

/**
 * Extract the collection name from a MongoDB JSON query, e.g.:
 *   {"find": "users", ...}  →  "users"
 */
export function extractCollectionName(sql: string): string | null {
  try {
    const obj = JSON.parse(sql) as Record<string, unknown>;
    const coll = obj.find ?? obj.aggregate ?? obj.insert ?? obj.update ?? obj.delete;
    return typeof coll === "string" ? coll : null;
  } catch {
    return null;
  }
}

/**
 * Build a MongoDB update command JSON for a single field edit.
 * Returns the JSON string to be executed.
 */
export function buildMongoUpdate(
  collection: string,
  db: string | null,
  pkField: string,
  pkValue: string,
  field: string,
  value: string,
): string {
  const cmd: Record<string, unknown> = {
    update: collection,
    updates: [
      {
        q: { [pkField]: pkValue },
        u: { $set: { [field]: value === "NULL" || value === "" ? null : value } },
        multi: false,
      },
    ],
  };
  if (db) cmd.db = db;
  return JSON.stringify(cmd);
}

/**
 * Build a MongoDB delete command JSON.
 */
export function buildMongoDelete(
  collection: string,
  db: string | null,
  pkField: string,
  pkValue: string,
): string {
  const cmd: Record<string, unknown> = {
    delete: collection,
    deletes: [{ q: { [pkField]: pkValue }, limit: 1 }],
  };
  if (db) cmd.db = db;
  return JSON.stringify(cmd);
}

export function sqlValue(raw: Row["values"][number]): string {
  if (!raw || raw.type === "null") return "NULL";
  if (raw.type === "bool") return raw.v ? "TRUE" : "FALSE";
  if (raw.type === "int" || raw.type === "float" || raw.type === "decimal") return String(raw.v);
  const str = displayValue(raw);
  return `'${str.replace(/'/g, "''")}'`;
}
