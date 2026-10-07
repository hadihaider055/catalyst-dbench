//! Oracle Database driver via the `oracle` crate (ODPI-C).
//!
//! Requires Oracle Instant Client at runtime (loaded dynamically); the app builds
//! without it. The `database` field is the service name, e.g. `FREEPDB1`.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    guard::is_sql_write,
    query::Query,
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::DatabaseSchema,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use oracle::sql_type::OracleType;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::sqlmeta::{build_objects, ColRow, FkRow};

const INSTANT_CLIENT_HINT: &str =
    "Oracle Instant Client is required: https://www.oracle.com/database/technologies/instant-client.html";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for an Oracle connection.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct OracleConfig {
    /// Hostname, or a full TNS descriptor `(DESCRIPTION=…)`.
    pub host: String,
    /// Listener port (default: 1521; TCPS usually 2484).
    pub port: u16,
    /// Service name.
    pub service: String,
    #[config(required)]
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 1521,
            service: "FREEPDB1".into(),
            username: "system".into(),
            password: None,
            tls: TlsConfig {
                mode: TlsMode::Disabled,
                ..Default::default()
            },
            mode: ConnectionMode::ReadWrite,
        }
    }
}

impl OracleConfig {
    /// Easy Connect string; `tcps://` when TLS is on (server cert verified by the client).
    fn connect_string(&self) -> String {
        if self.host.trim_start().starts_with('(') {
            return self.host.clone();
        }
        let scheme = if self.tls.mode == TlsMode::Disabled {
            ""
        } else {
            "tcps://"
        };
        format!("{scheme}{}:{}/{}", self.host, self.port, self.service)
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// Oracle driver.
#[derive(Debug, Default)]
pub struct OracleDriver;

impl Driver for OracleDriver {
    type Connection = OracleConnection;
    type Config = OracleConfig;

    fn name(&self) -> &'static str {
        "oracle"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Oracle
    }
    fn default_port(&self) -> Option<u16> {
        Some(1521)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        let fail =
            |e: String| CatalystError::connection_failed(DatabaseType::Oracle, &config.host, e);
        let (user, pass, target) = (
            config.username.clone(),
            config.password.clone().unwrap_or_default(),
            config.connect_string(),
        );

        let (conn, version) = blocking(move || {
            let mut conn = oracle::Connector::new(user, pass, target)
                .connect()
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("DPI-1047") {
                        format!("{msg}\n{INSTANT_CLIENT_HINT}")
                    } else {
                        msg
                    }
                })?;
            conn.set_autocommit(true);
            let version = conn.server_version().map(|(v, _)| v.to_string()).ok();
            Ok((conn, version))
        })
        .await
        .map_err(fail)?;

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Oracle,
            host: config.host.clone(),
            port: config.port,
            database: config.service.clone(),
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version: version,
            connected_at: chrono::Utc::now(),
        };
        tracing::info!(conn_id = %info.id, "Oracle connection established");
        Ok(OracleConnection {
            info,
            mode: config.mode,
            conn: Some(Arc::new(conn)),
        })
    }
}

/// Run blocking ODPI-C work off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> std::result::Result<T, String> + Send + 'static,
) -> std::result::Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// A live Oracle connection.
pub struct OracleConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    conn: Option<Arc<oracle::Connection>>,
}

impl OracleConnection {
    fn handle(&self) -> Result<Arc<oracle::Connection>> {
        self.conn
            .clone()
            .ok_or_else(|| CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            })
    }

    async fn run(&self, sql: String) -> std::result::Result<QueryResult, String> {
        let conn = self.handle().map_err(|e| e.to_string())?;
        let start = Instant::now();
        blocking(move || run_sync(&conn, &sql)).await.map(|mut r| {
            r.duration_ms = start.elapsed().as_millis() as u64;
            r
        })
    }
}

fn run_sync(conn: &oracle::Connection, sql: &str) -> std::result::Result<QueryResult, String> {
    let mut stmt = conn.statement(sql).build().map_err(|e| e.to_string())?;
    if !stmt.is_query() {
        stmt.execute(&[]).map_err(|e| e.to_string())?;
        let affected = stmt.row_count().map_err(|e| e.to_string())?;
        return Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(affected),
            duration_ms: 0,
            explain_plan: None,
        });
    }
    let rows = stmt.query(&[]).map_err(|e| e.to_string())?;
    let columns = rows
        .column_info()
        .iter()
        .map(|c| Column {
            name: c.name().to_string(),
            col_type: map_type(c.oracle_type()),
            nullable: c.nullable(),
            native_type: c.oracle_type().to_string(),
        })
        .collect();
    let mut out = Vec::new();
    for row in rows {
        let row = row.map_err(|e| e.to_string())?;
        out.push(Row {
            values: row.sql_values().iter().map(to_value).collect(),
        });
    }
    Ok(QueryResult {
        columns,
        rows: out,
        rows_affected: None,
        duration_ms: 0,
        explain_plan: None,
    })
}

/// Oracle rejects a trailing `;` on SQL, but PL/SQL blocks need their final `END;`.
fn trim_statement(sql: &str) -> String {
    let t = sql.trim();
    if t.to_ascii_uppercase().ends_with("END;") {
        t.to_string()
    } else {
        t.trim_end_matches(';').trim_end().to_string()
    }
}

impl Connection for OracleConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        // Oracle DDL auto-commits out of a READ ONLY transaction, so enforce client-side.
        if !self.mode.allows_writes() && is_sql_write(&query.text) {
            return Err(CatalystError::ReadOnlyViolation);
        }
        self.run(trim_statement(&query.text))
            .await
            .map_err(CatalystError::query_failed)
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let text = |v: Option<&Value>| match v {
            Some(Value::Text(s)) => s.clone(),
            Some(Value::Int(n)) => n.to_string(),
            Some(Value::Decimal(s)) => s.clone(),
            _ => String::new(),
        };
        let cols = self
            .run(COLUMNS_SQL.into())
            .await
            .map_err(CatalystError::SchemaError)?;
        let fks = self
            .run(FKS_SQL.into())
            .await
            .map_err(CatalystError::SchemaError)?;

        let col_rows = cols
            .rows
            .iter()
            .map(|r| {
                let v = &r.values;
                ColRow {
                    schema: text(v.first()),
                    table: text(v.get(1)),
                    is_view: text(v.get(2)) == "VIEW",
                    column: text(v.get(3)),
                    native_type: text(v.get(4)),
                    nullable: text(v.get(5)) == "Y",
                    default_value: None,
                    is_pk: text(v.get(6)) == "1",
                }
            })
            .collect();
        let fk_rows = fks
            .rows
            .iter()
            .map(|r| {
                let v = &r.values;
                FkRow {
                    name: text(v.first()),
                    schema: text(v.get(1)),
                    table: text(v.get(2)),
                    column: text(v.get(3)),
                    ref_table: text(v.get(4)),
                    ref_column: text(v.get(5)),
                }
            })
            .collect();

        Ok(DatabaseSchema {
            name: self.info.database.clone(),
            db_type: DatabaseType::Oracle,
            server_version: self.info.server_version.clone().unwrap_or_default(),
            objects: build_objects(col_rows, fk_rows),
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let conn = self.handle()?;
        let start = Instant::now();
        blocking(move || conn.ping().map_err(|e| e.to_string()))
            .await
            .map_err(|reason| CatalystError::ConnectionLost { reason })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        if let Some(conn) = self.conn.take() {
            blocking(move || conn.close().map_err(|e| e.to_string()))
                .await
                .map_err(|reason| CatalystError::ConnectionLost { reason })?;
        }
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.conn.is_some()
    }
    fn info(&self) -> &ConnectionInfo {
        &self.info
    }
    fn mode(&self) -> ConnectionMode {
        self.mode
    }
}

// ---------------------------------------------------------------------------
// Catalog queries (current schema only — ALL_* views include SYS otherwise)
// ---------------------------------------------------------------------------

const COLUMNS_SQL: &str = "\
SELECT c.OWNER, c.TABLE_NAME, CASE WHEN v.VIEW_NAME IS NULL THEN 'TABLE' ELSE 'VIEW' END, c.COLUMN_NAME,
       c.DATA_TYPE || CASE WHEN c.DATA_TYPE IN ('VARCHAR2','NVARCHAR2','CHAR','NCHAR','RAW')
                           THEN '(' || c.CHAR_LENGTH || ')' ELSE '' END,
       c.NULLABLE,
       CASE WHEN pk.COLUMN_NAME IS NULL THEN '0' ELSE '1' END
FROM ALL_TAB_COLUMNS c
LEFT JOIN ALL_VIEWS v ON v.OWNER = c.OWNER AND v.VIEW_NAME = c.TABLE_NAME
LEFT JOIN (SELECT cc.OWNER, cc.TABLE_NAME, cc.COLUMN_NAME
           FROM ALL_CONSTRAINTS k
           JOIN ALL_CONS_COLUMNS cc ON cc.OWNER = k.OWNER AND cc.CONSTRAINT_NAME = k.CONSTRAINT_NAME
           WHERE k.CONSTRAINT_TYPE = 'P') pk
  ON pk.OWNER = c.OWNER AND pk.TABLE_NAME = c.TABLE_NAME AND pk.COLUMN_NAME = c.COLUMN_NAME
WHERE c.OWNER = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AND c.TABLE_NAME NOT LIKE 'BIN$%'
ORDER BY c.TABLE_NAME, c.COLUMN_ID";

const FKS_SQL: &str = "\
SELECT a.CONSTRAINT_NAME, a.OWNER, a.TABLE_NAME, ac.COLUMN_NAME, r.TABLE_NAME, rc.COLUMN_NAME
FROM ALL_CONSTRAINTS a
JOIN ALL_CONS_COLUMNS ac ON ac.OWNER = a.OWNER AND ac.CONSTRAINT_NAME = a.CONSTRAINT_NAME
JOIN ALL_CONSTRAINTS r ON r.OWNER = a.R_OWNER AND r.CONSTRAINT_NAME = a.R_CONSTRAINT_NAME
JOIN ALL_CONS_COLUMNS rc ON rc.OWNER = r.OWNER AND rc.CONSTRAINT_NAME = r.CONSTRAINT_NAME AND rc.POSITION = ac.POSITION
WHERE a.CONSTRAINT_TYPE = 'R' AND a.OWNER = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')
ORDER BY a.CONSTRAINT_NAME, ac.POSITION";

// ---------------------------------------------------------------------------
// Type mapping
// ---------------------------------------------------------------------------

/// `NUMBER(p,0)` with p ≤ 18 fits an i64; other NUMBERs keep exact decimal text.
fn is_integer_number(t: &OracleType) -> bool {
    matches!(t, OracleType::Number(p, 0) if (1..=18).contains(p))
}

fn map_type(t: &OracleType) -> ColumnType {
    match t {
        t if is_integer_number(t) => ColumnType::Integer,
        OracleType::Number(..) => ColumnType::Decimal,
        OracleType::BinaryFloat | OracleType::BinaryDouble | OracleType::Float(_) => {
            ColumnType::Float
        }
        OracleType::Date
        | OracleType::Timestamp(_)
        | OracleType::TimestampTZ(_)
        | OracleType::TimestampLTZ(_) => ColumnType::Timestamp,
        OracleType::Raw(_) | OracleType::BLOB | OracleType::LongRaw => ColumnType::Bytes,
        OracleType::Boolean => ColumnType::Boolean,
        OracleType::Json => ColumnType::Json,
        _ => ColumnType::Text,
    }
}

fn to_value(v: &oracle::SqlValue) -> Value {
    if v.is_null().unwrap_or(true) {
        return Value::Null;
    }
    let text = || v.get::<String>().map_or(Value::Null, Value::Text);
    match v.oracle_type() {
        Ok(t) if is_integer_number(t) => v.get::<i64>().map_or_else(|_| text(), Value::Int),
        Ok(OracleType::Number(..)) => v.get::<String>().map_or(Value::Null, Value::Decimal),
        Ok(OracleType::BinaryFloat | OracleType::BinaryDouble | OracleType::Float(_)) => {
            v.get::<f64>().map_or_else(|_| text(), Value::Float)
        }
        Ok(OracleType::Date | OracleType::Timestamp(_)) => v
            .get::<chrono::NaiveDateTime>()
            .map_or_else(|_| text(), |dt| Value::Timestamp(dt.and_utc())),
        Ok(OracleType::TimestampTZ(_) | OracleType::TimestampLTZ(_)) => v
            .get::<chrono::DateTime<chrono::FixedOffset>>()
            .map_or_else(
                |_| text(),
                |dt| Value::Timestamp(dt.with_timezone(&chrono::Utc)),
            ),
        Ok(OracleType::Raw(_) | OracleType::BLOB | OracleType::LongRaw) => {
            v.get::<Vec<u8>>().map_or(Value::Null, Value::Bytes)
        }
        Ok(OracleType::Boolean) => v.get::<bool>().map_or_else(|_| text(), Value::Bool),
        _ => text(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_semicolons() {
        assert_eq!(trim_statement("SELECT 1 FROM dual;"), "SELECT 1 FROM dual");
        assert_eq!(trim_statement("BEGIN NULL; END;"), "BEGIN NULL; END;");
    }

    #[test]
    fn connect_strings() {
        let mut c = OracleConfig {
            host: "db".into(),
            port: 1521,
            service: "ORCL".into(),
            ..OracleConfig::default()
        };
        assert_eq!(c.connect_string(), "db:1521/ORCL");
        c.tls = TlsConfig::required();
        assert_eq!(c.connect_string(), "tcps://db:1521/ORCL");
        c.host = "(DESCRIPTION=(ADDRESS=(HOST=x)))".into();
        assert_eq!(c.connect_string(), c.host);
    }
}
