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
