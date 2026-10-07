// Types mirroring the Rust structs from dbench-core (serde JSON serialization)

export type DatabaseType =
  | "postgres" | "mysql" | "sqlite" | "cockroachdb" | "mssql" | "oracle"
  | "clickhouse" | "mongodb" | "fauna" | "couchdb" | "redis" | "dynamodb"
  | "etcd" | "cassandra" | "hbase" | "surrealdb" | "elasticsearch"
  | "influxdb" | "tigerbeetle";

export const DB_LABELS: Record<DatabaseType, string> = {
  postgres: "PostgreSQL", mysql: "MySQL / MariaDB", sqlite: "SQLite",
  cockroachdb: "CockroachDB", mssql: "SQL Server", oracle: "Oracle",
  clickhouse: "ClickHouse", mongodb: "MongoDB", fauna: "FaunaDB",
  couchdb: "CouchDB", redis: "Redis", dynamodb: "DynamoDB",
  etcd: "etcd", cassandra: "Cassandra", hbase: "HBase",
  surrealdb: "SurrealDB", elasticsearch: "Elasticsearch / OpenSearch",
  influxdb: "InfluxDB", tigerbeetle: "TigerBeetle",
};

export const DB_DEFAULTS: Partial<Record<DatabaseType, number>> = {
  postgres: 5432, mysql: 3306, mssql: 1433, oracle: 1521,
  mongodb: 27017, redis: 6379, clickhouse: 8123,
  cassandra: 9042, elasticsearch: 9200, influxdb: 8086, surrealdb: 8000,
};

export interface ConnectionInfo {
  id: string;
  db_type: DatabaseType;
  host: string;
  port: number;
  database: string;
  username: string;
  tls_active: boolean;
  ssh_tunnel: boolean;
  server_version?: string;
  connected_at: string;
}

export interface ConnectionPayload {
  name: string;
  db_type: DatabaseType;
  host: string;
  port?: number;
  database: string;
  username: string;
  password?: string;
  tls_enabled: boolean;
  tls_ca_path?: string;
  read_only: boolean;
  // SSH tunnel (optional)
  ssh_enabled?: boolean;
  ssh_host?: string;
  ssh_port?: number;
  ssh_username?: string;
  ssh_auth_method?: "agent" | "key";
  ssh_key_path?: string;
}

export type Value =
  | { type: "null" }
  | { type: "bool"; v: boolean }
  | { type: "int"; v: number }
  | { type: "float"; v: number }
  | { type: "decimal"; v: string }
  | { type: "text"; v: string }
  | { type: "bytes"; v: number[] }
  | { type: "date"; v: string }
  | { type: "time"; v: string }
  | { type: "timestamp"; v: string }
  | { type: "json"; v: unknown }
  | { type: "uuid"; v: string }
  | { type: "array"; v: Value[] }
  | { type: "object"; v: Record<string, unknown> };

export function displayValue(v: Value): string {
  if (!v) return "NULL";
  switch (v.type) {
    case "null": return "NULL";
    case "bool": return v.v ? "true" : "false";
    case "int":
    case "float":
    case "decimal": return String(v.v);
    case "text":
    case "date":
    case "time":
    case "uuid": return v.v;
    case "timestamp": return new Date(v.v).toLocaleString();
    case "bytes": return `\\x${v.v.map((b) => b.toString(16).padStart(2, "0")).join("")}`;
    case "json": return JSON.stringify(v.v);
    case "array": return `[${v.v.map(displayValue).join(", ")}]`;
    case "object": return JSON.stringify(v.v);
    default: return "?";
  }
}

export interface Column {
  name: string;
  col_type: string;
  nullable: boolean;
  native_type: string;
}

export interface Row {
  values: Value[];
}

export interface QueryResult {
  columns: Column[];
  rows: Row[];
  rows_affected?: number;
  duration_ms: number;
  explain_plan?: string;
}

export interface QueryPayload {
  connection_id: string;
  sql: string;
  params?: unknown[];
  timeout_ms?: number;
  explain?: boolean;
}

// Saved connection config (stored locally, no password)
export interface SavedConnection {
  id: string;
  name: string;
  db_type: DatabaseType;
  host: string;
  port: number;
  database: string;
  username: string;
  tls_enabled: boolean;
  tls_ca_path?: string;
  read_only: boolean;
  color?: string;
  // SSH tunnel config (stored without passphrase)
  ssh_enabled?: boolean;
  ssh_host?: string;
  ssh_port?: number;
  ssh_username?: string;
  ssh_auth_method?: "agent" | "key";
  ssh_key_path?: string;
}

export interface BatchStatementResult {
  sql: string;
  result?: QueryResult;
  error?: string;
}

export interface QueryTab {
  id: string;
  connection_id: string;
  connection_name: string;
  db_type: DatabaseType;
  title: string;
  sql: string;
  result?: QueryResult;
  error?: string;
  running: boolean;
  running_label?: string;
  batch_results?: BatchStatementResult[];
  kind?: "query" | "er_diagram" | "ddl";
  er_connection_id?: string;
  ddl_content?: string;
  ddl_object?: string;
}

export interface ForeignKeySchema {
  name: string;
  columns: string[];
  referenced_table: string;
  referenced_columns: string[];
  on_delete?: string;
  on_update?: string;
}

export interface HistoryEntry {
  id: string;
  sql: string;
  conn_id: string;
  conn_name: string;
  duration_ms: number;
  row_count: number;
  ts: number; // epoch ms
  error?: string;
}

export interface SavedQuery {
  id: string;
  name: string;
  sql: string;
  db_type?: DatabaseType;
  created_at: number; // epoch ms
}

// Schema types (mirrors dbench-core schema types)
export interface ColumnSchema {
  name: string;
  ordinal: number;
  native_type: string;
  nullable: boolean;
  default_value?: string;
  is_primary_key: boolean;
  is_unique: boolean;
  comment?: string;
}

export interface TableSchema {
  schema?: string;
  name: string;
  columns: ColumnSchema[];
  indexes: unknown[];
  foreign_keys: unknown[];
  row_count?: number;
  comment?: string;
}

export interface SchemaObject {
  kind: "table" | "view" | "collection" | "index" | "key_pattern" | "procedure";
  name?: string;
  schema?: string;
  database?: string;
  columns?: ColumnSchema[];
  [key: string]: unknown;
}

export interface DatabaseSchema {
  name: string;
  db_type: DatabaseType;
  server_version: string;
  objects: SchemaObject[];
}
