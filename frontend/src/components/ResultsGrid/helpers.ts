import type { Row } from "@/lib/types";
import { displayValue } from "@/lib/types";

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

export function exportCsv(headers: string[], rows: Row[]) {
  const lines = [
    headers.map(escapeCsvCell).join(","),
    ...rows.map((r) => r.values.map((v) => escapeCsvCell(displayValue(v ?? { type: "null" }))).join(",")),
  ];
  triggerDownload(new Blob([lines.join("\n")], { type: "text/csv;charset=utf-8;" }), "csv");
}

export function exportJson(headers: string[], rows: Row[]) {
  const data = rows.map((r) =>
    Object.fromEntries(headers.map((h, i) => [h, displayValue(r.values[i] ?? { type: "null" })]))
  );
  triggerDownload(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }), "json");
}

export function triggerDownload(blob: Blob, ext: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `export-${new Date().toISOString().slice(0, 19).replace(/:/g, "-")}.${ext}`;
  a.click();
  URL.revokeObjectURL(url);
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
