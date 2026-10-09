//! MySQL / MariaDB driver — uses `sqlx` with the `mysql` runtime.
//!
//! Compatible with: MySQL 5.7+, MySQL 8+, MariaDB 10.4+, PlanetScale.

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::{Query, QueryParam},
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::{ColumnSchema, DatabaseSchema, ForeignKeySchema, SchemaObject, TableSchema},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use serde::{Deserialize, Serialize};
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode, MySqlTypeInfo},
    Column as SqlxColumn, MySql, Pool, Row as SqlxRow, TypeInfo,
};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct MysqlConfig {
    #[config(required)]
    pub host: String,
    pub port: u16,
    #[config(required)]
    pub database: String,
    #[config(required)]
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
}

impl Default for MysqlConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 3306,
            database: String::new(),
            username: String::new(),
            password: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct MysqlDriver;

impl Driver for MysqlDriver {
    type Connection = MysqlConnection;
    type Config = MysqlConfig;

    fn name(&self) -> &'static str {
        "mysql"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Mysql
    }
    fn default_port(&self) -> Option<u16> {
        Some(3306)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        tracing::info!(
            host = %config.host,
            port = config.port,
            database = %config.database,
            "Connecting to MySQL"
        );

        let pool = build_pool(config).await?;

        // Fetch server version.
        let server_version: Option<String> = sqlx::query_scalar("SELECT VERSION()")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Mysql,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        tracing::info!(conn_id = %info.id, "MySQL connection established");

        Ok(MysqlConnection {
            info,
            mode: config.mode,
            alive: true,
            pool,
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct MysqlConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    pool: Pool<MySql>,
}

impl Connection for MysqlConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }

        // Read-only guard.
        // Scans every statement, ignoring comments/strings; also blocks `SET …` so a
        // session-level READ ONLY can't be switched back off.
        if !self.mode.allows_writes() && dbench_core::guard::is_sql_write(&query.text) {
            return Err(CatalystError::ReadOnlyViolation);
        }

        let start = Instant::now();

        let query_text = if query.explain {
            format!("EXPLAIN {}", query.text)
        } else {
            query.text.clone()
        };

        // Build sqlx query with bound parameters.
        let mut q = sqlx::query(&query_text);
        for param in &query.params {
            q = bind_param(q, param);
        }

        let upper = query_text.trim_start().to_uppercase();
        let is_dml = [
            "INSERT", "UPDATE", "DELETE", "TRUNCATE", "CREATE", "DROP", "ALTER",
        ]
        .iter()
        .any(|kw| upper.starts_with(kw));

        if is_dml && !query.explain {
            let res = q
                .execute(&self.pool)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: Some(res.rows_affected()),
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            });
        }

        let rows: Vec<MySqlRow> = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if query.explain {
            let plan = rows
                .iter()
                .map(|r| {
                    let n = r.len();
                    (0..n)
                        .filter_map(|i| r.try_get::<Option<String>, _>(i).ok().flatten())
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                duration_ms,
                explain_plan: Some(plan),
            });
        }

        if rows.is_empty() {
            // No row to read column metadata from: ask the server to describe the
            // statement so the grid can still show headers.
            let columns = sqlx::Executor::describe(&self.pool, query_text.as_str())
                .await
                .map(|d| {
                    d.columns()
                        .iter()
                        .map(|col| Column {
                            name: col.name().to_string(),
                            col_type: mysql_type_to_col_type(col.type_info()),
                            nullable: true,
                            native_type: col.type_info().name().to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            return Ok(QueryResult {
                columns,
                rows: vec![],
                rows_affected: None,
                duration_ms,
                explain_plan: None,
            });
        }

        let columns: Vec<Column> = rows[0]
            .columns()
            .iter()
            .map(|col| Column {
                name: col.name().to_string(),
                col_type: mysql_type_to_col_type(col.type_info()),
                nullable: true,
                native_type: col.type_info().name().to_string(),
            })
            .collect();

        let result_rows: Vec<Row> = rows
            .iter()
            .map(|row| Row {
                values: columns
                    .iter()
                    .enumerate()
                    .map(|(i, col)| extract_mysql_value(row, i, col))
                    .collect(),
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            duration_ms,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let db = &self.info.database;

        // Fetch all tables.
        let table_names: Vec<String> = sqlx::query_scalar(
            "SELECT CAST(TABLE_NAME AS CHAR) FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE'
             ORDER BY TABLE_NAME",
        )
        .bind(db)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

        // Fetch all foreign keys for this database in one query.
        let fk_raw: Vec<(String, String, String, String, String, String, String)> = sqlx::query_as(
            "SELECT
                    CAST(kcu.TABLE_NAME           AS CHAR),
                    CAST(kcu.CONSTRAINT_NAME       AS CHAR),
                    CAST(kcu.COLUMN_NAME           AS CHAR),
                    CAST(kcu.REFERENCED_TABLE_NAME AS CHAR),
                    CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR),
                    CAST(rc.DELETE_RULE            AS CHAR),
                    CAST(rc.UPDATE_RULE            AS CHAR)
                 FROM information_schema.KEY_COLUMN_USAGE kcu
                 JOIN information_schema.REFERENTIAL_CONSTRAINTS rc
                   ON  kcu.CONSTRAINT_NAME   = rc.CONSTRAINT_NAME
                   AND kcu.TABLE_SCHEMA      = rc.CONSTRAINT_SCHEMA
                 WHERE kcu.TABLE_SCHEMA = ?
                   AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
                 ORDER BY kcu.TABLE_NAME, kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
        )
        .bind(db)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        // Group by (table, constraint_name)
        let mut fk_map: std::collections::HashMap<
            String,
            std::collections::HashMap<String, ForeignKeySchema>,
        > = std::collections::HashMap::new();
        for (tbl, cname, col, ref_tbl, ref_col, on_del, on_upd) in fk_raw {
            let entry = fk_map
                .entry(tbl)
                .or_default()
                .entry(cname.clone())
                .or_insert_with(|| ForeignKeySchema {
                    name: cname,
                    columns: vec![],
                    referenced_table: ref_tbl,
                    referenced_columns: vec![],
                    on_delete: Some(on_del),
                    on_update: Some(on_upd),
                });
            entry.columns.push(col);
            entry.referenced_columns.push(ref_col);
        }

        let mut objects = Vec::new();

        for table_name in table_names {
            let cols: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT CAST(COLUMN_NAME AS CHAR), CAST(DATA_TYPE AS CHAR),
                        CAST(IS_NULLABLE AS CHAR), CAST(COLUMN_KEY AS CHAR)
                 FROM information_schema.COLUMNS
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
                 ORDER BY ORDINAL_POSITION",
            )
            .bind(db)
            .bind(&table_name)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

            let columns: Vec<ColumnSchema> = cols
                .into_iter()
                .enumerate()
                .map(|(i, (name, native_type, nullable, key))| ColumnSchema {
                    name,
                    ordinal: i as u32,
                    native_type,
                    nullable: nullable == "YES",
                    default_value: None,
                    is_primary_key: key == "PRI",
                    is_unique: key == "UNI" || key == "PRI",
                    comment: None,
                })
                .collect();

            let foreign_keys = fk_map
                .remove(&table_name)
                .map(|m| m.into_values().collect())
                .unwrap_or_default();

            objects.push(SchemaObject::Table(TableSchema {
                schema: Some(db.clone()),
                name: table_name,
                columns,
                indexes: vec![],
                foreign_keys,
                row_count: None,
                comment: None,
            }));
        }

        Ok(DatabaseSchema {
            name: db.clone(),
            db_type: DatabaseType::Mysql,
            server_version: self
                .info
                .server_version
                .clone()
                .unwrap_or_else(|| "MySQL".into()),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| CatalystError::ConnectionLost {
                reason: e.to_string(),
            })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        self.alive = false;
        self.pool.close().await;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive
    }
    fn info(&self) -> &ConnectionInfo {
        &self.info
    }
    fn mode(&self) -> ConnectionMode {
        self.mode
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn connect_options(config: &MysqlConfig) -> MySqlConnectOptions {
    // sqlx's `Required` encrypts but never checks the certificate (MITM-able);
    // `VerifyIdentity` validates the chain and hostname like the other drivers.
    let ssl_mode = match config.tls.mode {
        TlsMode::Disabled => MySqlSslMode::Disabled,
        TlsMode::Preferred => MySqlSslMode::Preferred,
        _ => MySqlSslMode::VerifyIdentity,
    };

    let mut opts = MySqlConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .database(&config.database)
        .username(&config.username)
        .ssl_mode(ssl_mode);
    if let Some(ca) = &config.tls.ca_cert_path {
        opts = opts.ssl_ca(ca);
    }

    if let Some(pass) = &config.password {
        opts = opts.password(pass);
    }

    opts
}

async fn build_pool(config: &MysqlConfig) -> Result<Pool<MySql>> {
    let read_only = !config.mode.allows_writes();
    MySqlPoolOptions::new()
        .max_connections(5)
        // Session settings are per connection: make *every* pooled connection
        // read-only, not just the one that happened to run the first statement.
        .after_connect(move |conn, _| {
            Box::pin(async move {
                if read_only {
                    sqlx::query("SET SESSION TRANSACTION READ ONLY")
                        .execute(conn)
                        .await?;
                }
                Ok(())
            })
        })
        .acquire_timeout(Duration::from_millis(
            config.connect_timeout_ms.unwrap_or(10_000),
        ))
        .connect_with(connect_options(config))
        .await
        .map_err(|e| {
            CatalystError::connection_failed(DatabaseType::Mysql, &config.host, e.to_string())
        })
}

fn bind_param<'q>(
    q: sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>,
    param: &'q QueryParam,
) -> sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments> {
    match param {
        QueryParam::Null => q.bind(Option::<String>::None),
        QueryParam::Bool(b) => q.bind(b),
        QueryParam::Int(i) => q.bind(i),
        QueryParam::Float(f) => q.bind(f),
        QueryParam::Text(s) => q.bind(s.as_str()),
        QueryParam::Bytes(b) => q.bind(b.as_slice()),
        QueryParam::Json(j) => q.bind(j.to_string()),
        QueryParam::Uuid(u) => q.bind(u.to_string()),
        QueryParam::Timestamp(s) => q.bind(s.as_str()),
    }
}

fn mysql_type_to_col_type(info: &MySqlTypeInfo) -> ColumnType {
    match info.name() {
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" | "TINYINT UNSIGNED"
        | "SMALLINT UNSIGNED" | "INT UNSIGNED" | "BIGINT UNSIGNED" => ColumnType::Integer,
        "FLOAT" | "DOUBLE" => ColumnType::Float,
        "DECIMAL" | "NUMERIC" => ColumnType::Decimal,
        "TINYINT(1)" | "BOOLEAN" | "BOOL" => ColumnType::Boolean,
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            ColumnType::Text
        }
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BINARY" | "VARBINARY" => {
            ColumnType::Bytes
        }
        "YEAR" | "BIT" => ColumnType::Integer,
        "DATE" => ColumnType::Date,
        "TIME" => ColumnType::Time,
        "DATETIME" | "TIMESTAMP" => ColumnType::Timestamp,
        "JSON" => ColumnType::Json,
        _ => ColumnType::Unknown,
    }
}

fn extract_mysql_value(row: &MySqlRow, idx: usize, col: &Column) -> Value {
    let v = extract_typed(row, idx, &col.col_type, &col.native_type);
    let is_null = row
        .try_get_raw(idx)
        .map_or(true, |r| sqlx::ValueRef::is_null(&r));
    if !v.is_null() || is_null {
        return v;
    }
    // Typed decode refused a non-NULL value: show its raw form rather than NULL.
    match row.try_get_unchecked::<Vec<u8>, _>(idx) {
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => Value::Text(t),
            Err(e) => Value::Bytes(e.into_bytes()),
        },
        Err(_) => Value::Null,
    }
}

fn extract_typed(row: &MySqlRow, idx: usize, col_type: &ColumnType, native: &str) -> Value {
    match col_type {
        ColumnType::Boolean => row
            .try_get::<bool, _>(idx)
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        // BIT(n) arrives as big-endian bytes.
        ColumnType::Integer if native == "BIT" => row
            .try_get_unchecked::<Vec<u8>, _>(idx)
            .map(|b| Value::Int(b.iter().fold(0i64, |acc, &x| (acc << 8) | i64::from(x))))
            .unwrap_or(Value::Null),
        ColumnType::Integer if native == "YEAR" => row
            .try_get_unchecked::<u16, _>(idx)
            .map(|y| Value::Int(y.into()))
            .unwrap_or(Value::Null),
        // Read unsigned first: as i64, values above i64::MAX wrap negative.
        ColumnType::Integer if native.ends_with("UNSIGNED") => row
            .try_get::<u64, _>(idx)
            .map(|v| i64::try_from(v).map_or_else(|_| Value::Decimal(v.to_string()), Value::Int))
            .unwrap_or(Value::Null),
        ColumnType::Integer => {
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return Value::Int(v);
            }
            if let Ok(v) = row.try_get::<i32, _>(idx) {
                return Value::Int(i64::from(v));
            }
            if let Ok(v) = row.try_get::<u64, _>(idx) {
                return Value::Int(v as i64);
            }
            Value::Null
        }
        ColumnType::Float => {
            if let Ok(v) = row.try_get::<f64, _>(idx) {
                return Value::Float(v);
            }
            row.try_get::<f32, _>(idx)
                .map(|v| Value::Float(f64::from(v)))
                .unwrap_or(Value::Null)
        }
        // DECIMAL is sent as text, but sqlx only decodes it into rust_decimal/bigdecimal.
        ColumnType::Decimal => row
            .try_get_unchecked::<String, _>(idx)
            .map(Value::Decimal)
            .unwrap_or(Value::Null),
        ColumnType::Bytes => row
            .try_get::<Vec<u8>, _>(idx)
            .map(Value::Bytes)
            .unwrap_or(Value::Null),
        ColumnType::Date => row
            .try_get::<chrono::NaiveDate, _>(idx)
            .map(|d| Value::Date(d.to_string()))
            .unwrap_or(Value::Null),
        // MySqlTime covers TIME's full range (-838:59:59..838:59:59), unlike NaiveTime.
        ColumnType::Time => row
            .try_get::<sqlx::mysql::types::MySqlTime, _>(idx)
            .map(|t| Value::Time(t.to_string()))
            .unwrap_or(Value::Null),
        ColumnType::Timestamp => {
            if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx) {
                return Value::Timestamp(v);
            }
            if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(idx) {
                return Value::Timestamp(v.and_utc());
            }
            Value::Null
        }
        ColumnType::Json => row
            .try_get::<serde_json::Value, _>(idx)
            .map(Value::Json)
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<String, _>(idx)
            .map(Value::Text)
            .unwrap_or(Value::Null),
    }
}
