import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type { DatabaseType } from "./types";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function dbColor(type: DatabaseType): string {
  const map: Partial<Record<DatabaseType, string>> = {
    postgres: "#336791", cockroachdb: "#336791",
    mysql: "#f29111",
    sqlite: "#003b57",
    mongodb: "#47a248",
    redis: "#dc382d",
    clickhouse: "#ffcc01",
    elasticsearch: "#f04e98",
    cassandra: "#1287b1",
    dynamodb: "#527fff",
    mssql: "#cc2927",
    oracle: "#f80000",
    surrealdb: "#ff00a0",
  };
  return map[type] ?? "#6a6a6a";
}

export function dbIcon(type: DatabaseType): string {
  const map: Partial<Record<DatabaseType, string>> = {
    postgres: "🐘", mysql: "🐬", sqlite: "🗃️",
    mongodb: "🍃", redis: "🔴", cockroachdb: "🪳",
    clickhouse: "🖱️", elasticsearch: "🔍", cassandra: "👁️",
    dynamodb: "⚡", surrealdb: "🌀", mssql: "🪟", oracle: "🔶",
  };
  return map[type] ?? "🗄️";
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function formatRowCount(n: number): string {
  if (n === 0) return "No rows";
  if (n === 1) return "1 row";
  return `${n.toLocaleString()} rows`;
}

export function generateId(): string {
  return crypto.randomUUID();
}

// `scheme://user:password@…` — userinfo of a connection URI.
const URI_WITH_PASSWORD = /^([a-z][a-z0-9+.-]*:\/\/)([^:@/?#]*):([^@/?#]*)@/i;
const URI_WITH_USER = /^([a-z][a-z0-9+.-]*:\/\/)([^:@/?#]*)@/i; // user may be empty (redis://:pw@…)

/** Split a connection URI into a password-free URI (safe to persist) and the password. */
export function splitUriPassword(uri: string): { uri: string; password?: string } {
  const m = URI_WITH_PASSWORD.exec(uri);
  if (!m) return { uri };
  let password = m[3];
  try {
    password = decodeURIComponent(password);
  } catch {
    /* not percent-encoded — keep as-is */
  }
  return { uri: uri.replace(URI_WITH_PASSWORD, "$1$2@"), password: password || undefined };
}

/** Put a password back into `scheme://user@…` — in memory only, right before connecting. */
export function withUriPassword(uri: string, password?: string): string {
  if (!password || URI_WITH_PASSWORD.test(uri)) return uri;
  return uri.replace(URI_WITH_USER, (_, scheme: string, user: string) => `${scheme}${user}:${encodeURIComponent(password)}@`);
}
