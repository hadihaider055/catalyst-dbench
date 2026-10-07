import type { DatabaseType } from "@/lib/types";

export const SQL_DB_TYPES: DatabaseType[] = [
  "postgres", "mysql", "sqlite", "cockroachdb", "mssql", "oracle", "clickhouse",
];

export const SNIPPETS: Record<string, { label: string; sql: string }[]> = {
  mysql: [
    { label: "Tables",      sql: "SHOW TABLES;" },
    { label: "Databases",   sql: "SHOW DATABASES;" },
    { label: "Columns",     sql: "SHOW COLUMNS FROM " },
    { label: "Indexes",     sql: "SHOW INDEXES FROM " },
    { label: "Processes",   sql: "SHOW PROCESSLIST;" },
    { label: "Status",      sql: "SHOW STATUS;" },
    { label: "Variables",   sql: "SHOW VARIABLES;" },
    { label: "Create Table",sql: "SHOW CREATE TABLE " },
  ],
  postgres: [
    { label: "Tables",      sql: "SELECT tablename FROM pg_tables WHERE schemaname = 'public';" },
    { label: "Databases",   sql: "SELECT datname FROM pg_database;" },
    { label: "Columns",     sql: "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = ''" },
    { label: "Indexes",     sql: "SELECT indexname, indexdef FROM pg_indexes WHERE tablename = ''" },
    { label: "Connections", sql: "SELECT * FROM pg_stat_activity;" },
    { label: "Table Size",  sql: "SELECT relname, pg_size_pretty(pg_total_relation_size(relid)) FROM pg_catalog.pg_statio_user_tables;" },
  ],
  sqlite: [
    { label: "Tables",  sql: "SELECT name FROM sqlite_master WHERE type='table';" },
    { label: "Schema",  sql: "SELECT sql FROM sqlite_master WHERE type='table' AND name=''" },
    { label: "Indexes", sql: "SELECT name FROM sqlite_master WHERE type='index';" },
  ],
  mongodb: [
    { label: "List DBs",    sql: '{"command": {"listDatabases": 1}}' },
    { label: "Find All",    sql: '{"find": "collection", "filter": {}, "limit": 100}' },
    { label: "Count",       sql: '{"aggregate": "collection", "pipeline": [{"$count": "total"}]}' },
    { label: "Server Info", sql: '{"command": {"buildInfo": 1}}' },
  ],
  redis: [
    { label: "All Keys", sql: "KEYS *" },
    { label: "DB Size",  sql: "DBSIZE" },
    { label: "Info",     sql: "INFO server" },
    { label: "Ping",     sql: "PING" },
    { label: "Get",      sql: "GET " },
    { label: "HGetAll",  sql: "HGETALL " },
  ],
  cockroachdb: [
    { label: "Tables",    sql: "SHOW TABLES;" },
    { label: "Databases", sql: "SHOW DATABASES;" },
  ],
  cassandra: [
    { label: "Keyspaces",   sql: "SELECT keyspace_name FROM system_schema.keyspaces;" },
    { label: "Tables",      sql: "SELECT table_name FROM system_schema.tables WHERE keyspace_name = '<keyspace>';" },
    { label: "Columns",     sql: "SELECT column_name, type, kind FROM system_schema.columns WHERE keyspace_name = '<keyspace>' AND table_name = '<table>';" },
    { label: "Select",      sql: "SELECT * FROM <keyspace>.<table> LIMIT 100;" },
    { label: "Create Table",sql: "CREATE TABLE <keyspace>.<table> (\n  id UUID PRIMARY KEY,\n  created_at TIMESTAMP\n);" },
    { label: "Partitions",  sql: "SELECT * FROM system.size_estimates WHERE keyspace_name = '<keyspace>';" },
  ],
  mssql: [
    { label: "Tables",    sql: "SELECT TABLE_SCHEMA, TABLE_NAME FROM INFORMATION_SCHEMA.TABLES ORDER BY 1, 2;" },
    { label: "Databases", sql: "SELECT name FROM sys.databases;" },
    { label: "Columns",   sql: "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_NAME = '<table>';" },
    { label: "Sessions",  sql: "SELECT session_id, login_name, status FROM sys.dm_exec_sessions WHERE is_user_process = 1;" },
  ],
  oracle: [
    { label: "Tables",    sql: "SELECT table_name FROM user_tables ORDER BY table_name" },
    { label: "Columns",   sql: "SELECT column_name, data_type, nullable FROM user_tab_columns WHERE table_name = '<TABLE>'" },
    { label: "Version",   sql: "SELECT banner FROM v$version" },
    { label: "Sessions",  sql: "SELECT sid, username, status FROM v$session WHERE username IS NOT NULL" },
  ],
  dynamodb: [
    { label: "Select",    sql: "SELECT * FROM \"<table>\" WHERE pk = '<value>'" },
    { label: "Index",     sql: "SELECT * FROM \"<table>\".\"<index>\" WHERE gsi_pk = '<value>'" },
    { label: "Insert",    sql: "INSERT INTO \"<table>\" VALUE {'pk': '<value>', 'attr': 1}" },
    { label: "Update",    sql: "UPDATE \"<table>\" SET attr = 2 WHERE pk = '<value>'" },
  ],
  elasticsearch: [
    { label: "Indices",   sql: "GET /_cat/indices?format=json&v" },
    { label: "Health",    sql: "GET /_cluster/health" },
    { label: "Search",    sql: "GET /<index>/_search\n{\n  \"size\": 20,\n  \"query\": { \"match_all\": {} }\n}" },
    { label: "Mapping",   sql: "GET /<index>/_mapping" },
    { label: "SQL",       sql: "SELECT * FROM \"<index>\" LIMIT 20" },
  ],
  surrealdb: [
    { label: "Info",      sql: "INFO FOR DB;" },
    { label: "Select",    sql: "SELECT * FROM <table> LIMIT 100;" },
    { label: "Table info",sql: "INFO FOR TABLE <table>;" },
    { label: "Create",    sql: "CREATE <table> CONTENT { name: 'example' };" },
  ],
  clickhouse: [
    { label: "Tables",      sql: "SHOW TABLES;" },
    { label: "Databases",   sql: "SHOW DATABASES;" },
    { label: "Columns",     sql: "DESCRIBE TABLE " },
    { label: "Processes",   sql: "SELECT * FROM system.processes;" },
    { label: "Table Size",  sql: "SELECT table, formatReadableSize(sum(bytes)) AS size FROM system.parts WHERE active GROUP BY table ORDER BY sum(bytes) DESC;" },
    { label: "Create Table",sql: "SHOW CREATE TABLE " },
    { label: "Mutations",   sql: "SELECT * FROM system.mutations WHERE is_done = 0;" },
  ],
};

/** "First 100 rows" in each dialect (only MySQL-family/Postgres/SQLite/CH/CQL/ES/Surreal accept LIMIT). */
export function selectRowsSql(dbType: DatabaseType, table: string): string {
  switch (dbType) {
    case "mssql":
      return `SELECT TOP 100 * FROM ${table};`;
    case "oracle":
      return `SELECT * FROM ${table} FETCH FIRST 100 ROWS ONLY`;
    case "dynamodb":
      return `SELECT * FROM "${table.replace(/"/g, '""')}"`;
    default:
      return `SELECT * FROM ${table} LIMIT 100;`;
  }
}

export function getSnippets(dbType: DatabaseType) {
  return SNIPPETS[dbType] ?? SNIPPETS["mysql"] ?? [];
}

// Full list of SQL keywords used in intellisense
export const SQL_KEYWORDS = [
  "SELECT", "FROM", "WHERE", "JOIN", "LEFT JOIN", "RIGHT JOIN", "INNER JOIN",
  "OUTER JOIN", "FULL JOIN", "CROSS JOIN", "ON", "GROUP BY", "ORDER BY",
  "HAVING", "LIMIT", "OFFSET", "AS", "DISTINCT", "INSERT INTO", "VALUES",
  "UPDATE", "SET", "DELETE FROM", "CREATE TABLE", "DROP TABLE", "ALTER TABLE",
  "ADD COLUMN", "DROP COLUMN", "TRUNCATE", "BEGIN", "COMMIT", "ROLLBACK",
  "EXPLAIN", "ANALYZE", "AND", "OR", "NOT", "IN", "NOT IN", "EXISTS",
  "LIKE", "ILIKE", "IS NULL", "IS NOT NULL", "BETWEEN", "CASE", "WHEN",
  "THEN", "ELSE", "END", "COUNT", "SUM", "AVG", "MAX", "MIN", "COALESCE",
  "NULLIF", "CAST", "UNION", "UNION ALL", "INTERSECT", "EXCEPT",
  "ASC", "DESC", "NULLS FIRST", "NULLS LAST", "WITH", "RETURNING",
  "PRIMARY KEY", "FOREIGN KEY", "REFERENCES", "UNIQUE", "NOT NULL", "DEFAULT",
  "INDEX", "VIEW", "TRIGGER", "PROCEDURE", "FUNCTION",
];
