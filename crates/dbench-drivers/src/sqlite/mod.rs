//! SQLite driver — uses `rusqlite` (bundled) via `spawn_blocking`.
//!
//! Supports file-based and in-memory (`:memory:`) databases.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::Query,
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::{ColumnSchema, DatabaseSchema, SchemaObject, TableSchema},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a SQLite connection.
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Path to the SQLite file, or `:memory:` for an in-memory database.
    #[config(required)]
    pub path: String,
    pub mode: ConnectionMode,
    #[serde(default = "default_true")]
    pub wal_mode: bool,
}

fn default_true() -> bool { true }

impl Default for SqliteConfig {
    fn default() -> Self {
        Self { path: ":memory:".into(), mode: ConnectionMode::ReadWrite, wal_mode: true }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct SqliteDriver;

impl Driver for SqliteDriver {
    type Connection = SqliteConnection;
    type Config = SqliteConfig;

    fn name(&self) -> &'static str { "sqlite" }
    fn database_type(&self) -> DatabaseType { DatabaseType::Sqlite }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        tracing::info!(path = %config.path, "Opening SQLite database");

        let path = config.path.clone();
        let mode = config.mode;
        let wal_mode = config.wal_mode;

        let inner = tokio::task::spawn_blocking(move || -> Result<rusqlite::Connection> {
            let flags = if mode.allows_writes() {
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                    | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            } else {
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            };

            let conn = rusqlite::Connection::open_with_flags(&path, flags)
                .map_err(|e| CatalystError::connection_failed(
                    DatabaseType::Sqlite, &path, e.to_string(),
                ))?;

            conn.pragma_update(None, "foreign_keys", true)
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;
            conn.pragma_update(None, "busy_timeout", 5000)
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            if wal_mode && mode.allows_writes() {
                conn.pragma_update(None, "journal_mode", "WAL")
                    .map_err(|e| CatalystError::query_failed(e.to_string()))?;
            }

            Ok(conn)
        }).await
            .map_err(|e| CatalystError::Internal(e.to_string()))??;

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Sqlite,
            host: "localhost".into(),
            port: 0,
            database: config.path.clone(),
            username: String::new(),
            tls_active: false,
            ssh_tunnel: false,
            server_version: Some(rusqlite::version().to_string()),
            connected_at: chrono::Utc::now(),
        };

        Ok(SqliteConnection {
            info,
            mode: config.mode,
            alive: true,
            inner: Arc::new(Mutex::new(inner)),
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct SqliteConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    inner: Arc<Mutex<rusqlite::Connection>>,
}

impl Connection for SqliteConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost { reason: "connection is closed".into() });
        }

        if !self.mode.allows_writes() {
            let upper = query.text.trim_start().to_uppercase();
            let writes = ["INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE"];
            if writes.iter().any(|kw| upper.starts_with(kw)) {
                return Err(CatalystError::ReadOnlyViolation);
            }
        }

        let query_text = if query.explain {
            format!("EXPLAIN QUERY PLAN {}", query.text)
        } else {
            query.text.clone()
        };

        let inner = Arc::clone(&self.inner);
        let start = Instant::now();

        let result = tokio::task::spawn_blocking(move || -> Result<QueryResult> {
            let conn = inner.lock()
                .map_err(|_| CatalystError::ConnectionLost { reason: "mutex poisoned".into() })?;

            let mut stmt = conn.prepare(&query_text)
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            let col_count = stmt.column_count();
            let col_meta: Vec<(String, String)> = stmt.columns().into_iter().map(|c| {
                let name = c.name().to_string();
                let decl = c.decl_type().unwrap_or("TEXT").to_uppercase();
                (name, decl)
            }).collect();
            let columns: Vec<Column> = col_meta.iter().map(|(name, decl)| {
                let col_type = sqlite_type_to_col_type(decl);
                Column { name: name.clone(), col_type, nullable: true, native_type: decl.clone() }
            }).collect();

            let mut rows_out = vec![];
            let mut stmt2 = conn.prepare(&query_text)
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            let mut query_rows = stmt2.query([])
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            while let Some(row) = query_rows.next()
                .map_err(|e| CatalystError::query_failed(e.to_string()))? {

                let values: Vec<Value> = (0..col_count).map(|i| {
                    match row.get_ref(i) {
                        Ok(ValueRef::Null) => Value::Null,
                        Ok(ValueRef::Integer(n)) => Value::Int(n),
                        Ok(ValueRef::Real(f)) => Value::Float(f),
                        Ok(ValueRef::Text(s)) => {
                            Value::Text(String::from_utf8_lossy(s).into_owned())
                        }
                        Ok(ValueRef::Blob(b)) => Value::Bytes(b.to_vec()),
                        Err(_) => Value::Null,
                    }
                }).collect();

                rows_out.push(Row { values });
            }

            // For DML: rows_affected via conn.changes().
            let rows_affected = if rows_out.is_empty() {
                let upper = query_text.trim_start().to_uppercase();
                let is_dml = ["INSERT", "UPDATE", "DELETE"]
                    .iter().any(|kw| upper.starts_with(kw));
                if is_dml { Some(conn.changes()) } else { None }
            } else {
                None
            };

            Ok(QueryResult {
                columns,
                rows: rows_out,
                rows_affected,
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            })
        }).await
            .map_err(|e| CatalystError::Internal(e.to_string()))??;

        Ok(result)
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let inner = Arc::clone(&self.inner);
        let db_name = self.info.database.clone();

        let objects = tokio::task::spawn_blocking(move || -> Result<Vec<SchemaObject>> {
            let conn = inner.lock()
                .map_err(|_| CatalystError::ConnectionLost { reason: "mutex poisoned".into() })?;

            let mut stmt = conn.prepare(
                "SELECT name, type FROM sqlite_schema WHERE type IN ('table','view') \
                 AND name NOT LIKE 'sqlite_%' ORDER BY type, name"
            ).map_err(|e| CatalystError::SchemaError(e.to_string()))?;

            let mut objects = vec![];
            let mut rows = stmt.query([])
                .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

            while let Some(row) = rows.next()
                .map_err(|e| CatalystError::SchemaError(e.to_string()))? {
                let name: String = row.get(0).unwrap_or_default();
                let kind: String = row.get(1).unwrap_or_default();

                if kind == "table" {
                    // Fetch columns via PRAGMA table_info.
                    let mut col_stmt = conn.prepare(
                        &format!("PRAGMA table_info(\"{}\")", name)
                    ).map_err(|e| CatalystError::SchemaError(e.to_string()))?;

                    let mut col_rows = col_stmt.query([])
                        .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

                    let mut columns = vec![];
                    while let Some(cr) = col_rows.next()
                        .map_err(|e| CatalystError::SchemaError(e.to_string()))? {
                        let cid: i64 = cr.get(0).unwrap_or(0);
                        let col_name: String = cr.get(1).unwrap_or_default();
                        let col_type: String = cr.get(2).unwrap_or_else(|_| "TEXT".to_string());
                        let not_null: bool = cr.get::<_, i64>(3).unwrap_or(0) != 0;
                        let dflt: Option<String> = cr.get(4).ok();
                        let is_pk: bool = cr.get::<_, i64>(5).unwrap_or(0) != 0;

                        columns.push(ColumnSchema {
                            name: col_name,
                            ordinal: cid as u32,
                            native_type: col_type,
                            nullable: !not_null,
                            default_value: dflt,
                            is_primary_key: is_pk,
                            is_unique: false,
                            comment: None,
                        });
                    }

                    // Row count estimate.
                    let row_count: Option<u64> = conn
                        .query_row(
                            &format!("SELECT COUNT(*) FROM \"{}\"", name),
                            [],
                            |r| r.get::<_, i64>(0),
                        )
                        .ok()
                        .map(|n| n as u64);

                    objects.push(SchemaObject::Table(TableSchema {
                        schema: None,
                        name,
                        columns,
                        indexes: vec![],
                        foreign_keys: vec![],
                        row_count,
                        comment: None,
                    }));
                }
            }

            Ok(objects)
        }).await
            .map_err(|e| CatalystError::Internal(e.to_string()))??;

        Ok(DatabaseSchema {
            name: db_name,
            db_type: DatabaseType::Sqlite,
            server_version: rusqlite::version().to_string(),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        if self.alive { Ok(Duration::ZERO) } else {
            Err(CatalystError::ConnectionLost { reason: "connection closed".into() })
        }
    }

    async fn close(mut self) -> Result<()> {
        self.alive = false;
        Ok(())
    }

    fn is_alive(&self) -> bool { self.alive }
    fn info(&self) -> &ConnectionInfo { &self.info }
    fn mode(&self) -> ConnectionMode { self.mode }
}

fn sqlite_type_to_col_type(decl: &str) -> ColumnType {
    if decl.contains("INT") { return ColumnType::Integer; }
    if decl.contains("REAL") || decl.contains("FLOAT") || decl.contains("DOUBLE") {
        return ColumnType::Float;
    }
    if decl.contains("NUMERIC") || decl.contains("DECIMAL") { return ColumnType::Decimal; }
    if decl.contains("BOOL") { return ColumnType::Boolean; }
    if decl.contains("BLOB") { return ColumnType::Bytes; }
    if decl.contains("DATE") { return ColumnType::Date; }
    if decl.contains("TIME") { return ColumnType::Time; }
    if decl.contains("JSON") { return ColumnType::Json; }
    ColumnType::Text
}
