//! PostgreSQL driver — uses `tokio-postgres` with a background connection task.
//!
//! Compatible with: PostgreSQL 12+, CockroachDB, Supabase, Neon.

mod schema;

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::{Query, QueryParam},
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::DatabaseSchema,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a PostgreSQL connection.
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
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
    pub application_name: Option<String>,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 5432,
            database: String::new(),
            username: String::new(),
            password: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
            application_name: Some("dbench".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct PostgresDriver;

impl Driver for PostgresDriver {
    type Connection = PostgresConnection;
    type Config = PostgresConfig;

    fn name(&self) -> &'static str { "postgres" }
    fn database_type(&self) -> DatabaseType { DatabaseType::Postgres }
    fn default_port(&self) -> Option<u16> { Some(5432) }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        tracing::info!(
            host = %config.host, port = config.port,
            database = %config.database, username = %config.username,
            "Connecting to PostgreSQL"
        );

        let mut pg_config = tokio_postgres::Config::new();
        pg_config
            .host(&config.host)
            .port(config.port)
            .dbname(&config.database)
            .user(&config.username)
            .application_name(config.application_name.as_deref().unwrap_or("dbench"));

        if let Some(pass) = &config.password {
            pg_config.password(pass.as_str());
        }

        if let Some(ms) = config.connect_timeout_ms {
            pg_config.connect_timeout(std::time::Duration::from_millis(ms));
        }

        let (client, connection) = pg_config
            .connect(tokio_postgres::NoTls)
            .await
            .map_err(|e| CatalystError::connection_failed(
                DatabaseType::Postgres, &config.host, e.to_string(),
            ))?;

        // Drive the connection in the background.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("PostgreSQL connection error: {}", e);
            }
        });

        // Apply read-only session if needed.
        if !config.mode.allows_writes() {
            client
                .execute("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY", &[])
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;
        }

        // Fetch server version.
        let server_version = client
            .query_one("SELECT version()", &[])
            .await
            .ok()
            .and_then(|r| r.try_get::<_, String>(0).ok())
            .map(|v| v.split(' ').nth(1).unwrap_or("unknown").to_string());

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Postgres,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        tracing::info!(conn_id = %info.id, "PostgreSQL connection established");

        Ok(PostgresConnection {
            info,
            mode: config.mode,
            alive: true,
            client,
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct PostgresConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    client: tokio_postgres::Client,
}

impl Connection for PostgresConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost { reason: "connection is closed".into() });
        }

        // Engine-level read-only guard.
        if !self.mode.allows_writes() {
            let upper = query.text.trim_start().to_uppercase();
            let writes = ["INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE"];
            if writes.iter().any(|kw| upper.starts_with(kw)) {
                return Err(CatalystError::ReadOnlyViolation);
            }
        }

        let start = Instant::now();

        let query_text = if query.explain {
            format!("EXPLAIN ANALYZE {}", query.text)
        } else {
            query.text.clone()
        };

        // Convert QueryParams to boxed ToSql values.
        let owned_params = to_pg_params(&query.params);
        let param_refs: Vec<&(dyn ToSql + Sync)> = owned_params
            .iter()
            .map(|p| p.as_ref() as &(dyn ToSql + Sync))
            .collect();

        let stmt = self.client
            .prepare(&query_text)
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?;

        let columns: Vec<Column> = stmt.columns().iter().map(|col| Column {
            name: col.name().to_string(),
            col_type: pg_type_to_col_type(col.type_()),
            nullable: true,
            native_type: col.type_().name().to_string(),
        }).collect();

        // DML vs SELECT dispatch.
        let upper = query_text.trim_start().to_uppercase();
        let is_dml = ["INSERT", "UPDATE", "DELETE", "TRUNCATE", "CREATE", "DROP", "ALTER"]
            .iter().any(|kw| upper.starts_with(kw));

        if is_dml && !query.explain {
            let rows_affected = self.client
                .execute(&stmt, &param_refs)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: Some(rows_affected),
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            });
        }

        let rows = self.client
            .query(&stmt, &param_refs)
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if query.explain {
            let plan = rows.iter()
                .filter_map(|r| r.try_get::<_, String>(0).ok())
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

        let result_rows: Vec<Row> = rows.iter().map(|row| Row {
            values: columns.iter().enumerate().map(|(i, col)| {
                extract_pg_value(row, i, &col.col_type)
            }).collect(),
        }).collect();

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            duration_ms,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        schema::inspect(&self.client, &self.info.database).await
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.client.simple_query("SELECT 1").await
            .map_err(|e| CatalystError::ConnectionLost { reason: e.to_string() })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        self.alive = false;
        Ok(())
    }

    fn is_alive(&self) -> bool { self.alive }
    fn info(&self) -> &ConnectionInfo { &self.info }
    fn mode(&self) -> ConnectionMode { self.mode }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_pg_params(params: &[QueryParam]) -> Vec<Box<dyn ToSql + Sync + Send>> {
    params.iter().map(|p| -> Box<dyn ToSql + Sync + Send> {
        match p {
            QueryParam::Null      => Box::new(Option::<String>::None),
            QueryParam::Bool(b)   => Box::new(*b),
            QueryParam::Int(i)    => Box::new(*i),
            QueryParam::Float(f)  => Box::new(*f),
            QueryParam::Text(s)   => Box::new(s.clone()),
            QueryParam::Bytes(b)  => Box::new(b.clone()),
            QueryParam::Json(j)   => Box::new(j.clone()),
            QueryParam::Uuid(u)   => Box::new(*u),
            QueryParam::Timestamp(s) => Box::new(s.clone()),
        }
    }).collect()
}

fn pg_type_to_col_type(pg: &tokio_postgres::types::Type) -> ColumnType {
    use tokio_postgres::types::Type;
    match *pg {
        Type::BOOL => ColumnType::Boolean,
        Type::INT2 | Type::INT4 | Type::INT8 | Type::OID => ColumnType::Integer,
        Type::FLOAT4 | Type::FLOAT8 => ColumnType::Float,
        Type::NUMERIC => ColumnType::Decimal,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => ColumnType::Text,
        Type::BYTEA => ColumnType::Bytes,
        Type::DATE => ColumnType::Date,
        Type::TIME | Type::TIMETZ => ColumnType::Time,
        Type::TIMESTAMP | Type::TIMESTAMPTZ => ColumnType::Timestamp,
        Type::JSON | Type::JSONB => ColumnType::Json,
        Type::UUID => ColumnType::Uuid,
        _ => ColumnType::Unknown,
    }
}

fn extract_pg_value(row: &tokio_postgres::Row, idx: usize, col_type: &ColumnType) -> Value {
    match col_type {
        ColumnType::Boolean => {
            row.try_get::<_, bool>(idx).map(Value::Bool).unwrap_or(Value::Null)
        }
        ColumnType::Integer => {
            if let Ok(v) = row.try_get::<_, i64>(idx) { return Value::Int(v); }
            if let Ok(v) = row.try_get::<_, i32>(idx) { return Value::Int(i64::from(v)); }
            if let Ok(v) = row.try_get::<_, i16>(idx) { return Value::Int(i64::from(v)); }
            row.try_get::<_, u32>(idx).map(|v| Value::Int(i64::from(v))).unwrap_or(Value::Null)
        }
        ColumnType::Float => {
            if let Ok(v) = row.try_get::<_, f64>(idx) { return Value::Float(v); }
            row.try_get::<_, f32>(idx).map(|v| Value::Float(f64::from(v))).unwrap_or(Value::Null)
        }
        ColumnType::Decimal => {
            row.try_get::<_, String>(idx).map(Value::Decimal).unwrap_or(Value::Null)
        }
        ColumnType::Bytes => {
            row.try_get::<_, Vec<u8>>(idx).map(Value::Bytes).unwrap_or(Value::Null)
        }
        ColumnType::Date => {
            row.try_get::<_, chrono::NaiveDate>(idx)
                .map(|d| Value::Date(d.to_string()))
                .unwrap_or(Value::Null)
        }
        ColumnType::Time => {
            row.try_get::<_, chrono::NaiveTime>(idx)
                .map(|t| Value::Time(t.to_string()))
                .unwrap_or(Value::Null)
        }
        ColumnType::Timestamp => {
            if let Ok(v) = row.try_get::<_, chrono::DateTime<chrono::Utc>>(idx) {
                return Value::Timestamp(v);
            }
            row.try_get::<_, chrono::NaiveDateTime>(idx)
                .map(|dt| Value::Timestamp(dt.and_utc()))
                .unwrap_or(Value::Null)
        }
        ColumnType::Json => {
            row.try_get::<_, serde_json::Value>(idx).map(Value::Json).unwrap_or(Value::Null)
        }
        ColumnType::Uuid => {
            row.try_get::<_, uuid::Uuid>(idx).map(Value::Uuid).unwrap_or(Value::Null)
        }
        _ => {
            row.try_get::<_, String>(idx).map(Value::Text).unwrap_or(Value::Null)
        }
    }
}
