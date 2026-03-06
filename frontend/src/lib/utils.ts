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
  };
  return map[type] ?? "#6a6a6a";
}

export function dbIcon(type: DatabaseType): string {
  const map: Partial<Record<DatabaseType, string>> = {
    postgres: "🐘", mysql: "🐬", sqlite: "🗃️",
    mongodb: "🍃", redis: "🔴", cockroachdb: "🪳",
    clickhouse: "🖱️", elasticsearch: "🔍", cassandra: "👁️",
    dynamodb: "⚡", surrealdb: "🌀",
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
