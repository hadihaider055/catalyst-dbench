//! ClickHouse driver implementation.
//!
//! Uses the ClickHouse HTTP interface (port 8123 by default).
//! All queries are sent as POST requests with `FORMAT JSONCompact` appended
//! so results come back as structured JSON.

use std::time::{Duration, Instant};

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
use dbench_security::tls::{TlsConfig, TlsMode};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for connecting to ClickHouse via HTTP.
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct ClickhouseConfig {
    pub host: String,
    /// HTTP port (default: 8123; HTTPS: 8443).
    pub port: u16,
    pub database: String,
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
}

impl Default for ClickhouseConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8123,
            database: "default".into(),
            username: "default".into(),
            password: None,
            tls: TlsConfig { mode: TlsMode::Disabled, ..Default::default() },
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

pub struct ClickhouseDriver;

impl Driver for ClickhouseDriver {
    type Connection = ClickhouseConnection;
    type Config = ClickhouseConfig;

    fn name(&self) -> &'static str { "clickhouse" }
    fn database_type(&self) -> DatabaseType { DatabaseType::Clickhouse }
    fn default_port(&self) -> Option<u16> { Some(8123) }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        let timeout = Duration::from_millis(config.connect_timeout_ms.unwrap_or(10_000));
        let client = build_client(timeout)?;
        let base_url = build_base_url(config);

        // Ping: run a trivial query to verify the connection.
        let pong = send_raw(
            &client,
            &base_url,
            &config.username,
            config.password.as_deref(),
            "SELECT 1 FORMAT JSONCompact",
        )
        .await
        .map_err(|e| CatalystError::connection_failed(DatabaseType::Clickhouse, &config.host, e))?;

        let server_version = fetch_scalar(
            &client,
            &base_url,
            &config.username,
            config.password.as_deref(),
            "SELECT version()",
        )
        .await
        .ok();

        let _ = pong;

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Clickhouse,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        tracing::info!(conn_id = %info.id, "ClickHouse connection established");

        Ok(ClickhouseConnection {
            info,
            mode: config.mode,
            alive: true,
            client,
            base_url,
            username: config.username.clone(),
            password: config.password.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct ClickhouseConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    client: Client,
    base_url: String,
    username: String,
    password: Option<String>,
}

impl Connection for ClickhouseConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost { reason: "connection is closed".into() });
        }

        // Write guard.
        if !self.mode.allows_writes() {
            let upper = query.text.trim_start().to_uppercase();
            for kw in &["INSERT", "CREATE", "DROP", "TRUNCATE", "ALTER", "RENAME", "OPTIMIZE"] {
                if upper.starts_with(kw) {
                    return Err(CatalystError::ReadOnlyViolation);
                }
            }
        }

        let start = Instant::now();
        // Append FORMAT JSONCompact unless the query already specifies a FORMAT.
        let sql = if query.text.to_uppercase().contains("FORMAT ") {
            query.text.clone()
        } else {
            format!("{} FORMAT JSONCompact", query.text.trim_end().trim_end_matches(';'))
        };

        let raw = send_raw(&self.client, &self.base_url, &self.username, self.password.as_deref(), &sql)
            .await
            .map_err(|e| CatalystError::query_failed(e))?;

        parse_response(&raw, start.elapsed().as_millis() as u64)
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let db = &self.info.database;

        // Columns for all user tables in the connected database.
        let col_sql = format!(
            "SELECT table, name, type, is_in_primary_key, is_in_sorting_key \
             FROM system.columns \
             WHERE database = '{db}' \
             ORDER BY table, position \
             FORMAT JSONCompact"
        );

        let raw = send_raw(&self.client, &self.base_url, &self.username, self.password.as_deref(), &col_sql)
            .await
            .map_err(|e| CatalystError::SchemaError(e))?;

        let resp: ChResponse = serde_json::from_str(&raw)
            .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

        // Group columns by table name.
        let mut tables: std::collections::BTreeMap<String, Vec<ColumnSchema>> = std::collections::BTreeMap::new();
        for (ordinal, row) in resp.data.iter().enumerate() {
            if row.len() < 5 { continue; }
            let table_name = row[0].as_str().unwrap_or("").to_string();
            let col_name = row[1].as_str().unwrap_or("").to_string();
            let native_type = row[2].as_str().unwrap_or("").to_string();
            let is_pk = row[3].as_u64().unwrap_or(0) == 1;

            let nullable = native_type.starts_with("Nullable(");
            let base_type = if nullable {
                native_type
                    .strip_prefix("Nullable(")
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or(&native_type)
                    .to_string()
            } else {
                native_type.clone()
            };

            tables.entry(table_name).or_default().push(ColumnSchema {
                name: col_name,
                ordinal: ordinal as u32,
                native_type: base_type,
                nullable,
                default_value: None,
                is_primary_key: is_pk,
                is_unique: is_pk,
                comment: None,
            });
        }

        // Row counts via system.tables.
        let counts_sql = format!(
            "SELECT name, total_rows FROM system.tables WHERE database = '{db}' FORMAT JSONCompact"
        );
        let mut row_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        if let Ok(raw2) = send_raw(&self.client, &self.base_url, &self.username, self.password.as_deref(), &counts_sql).await {
            if let Ok(resp2) = serde_json::from_str::<ChResponse>(&raw2) {
                for row in resp2.data {
                    if row.len() >= 2 {
                        let name = row[0].as_str().unwrap_or("").to_string();
                        let count = row[1].as_u64().unwrap_or(0);
                        row_counts.insert(name, count);
                    }
                }
            }
        }

        let objects: Vec<SchemaObject> = tables
            .into_iter()
            .map(|(name, columns)| {
                SchemaObject::Table(TableSchema {
                    schema: Some(db.clone()),
                    row_count: row_counts.get(&name).copied(),
                    name,
                    columns,
                    indexes: vec![],
                    foreign_keys: vec![],
                    comment: None,
                })
            })
            .collect();

        Ok(DatabaseSchema {
            name: db.clone(),
            db_type: DatabaseType::Clickhouse,
            server_version: self.info.server_version.clone().unwrap_or_else(|| "ClickHouse".into()),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        send_raw(&self.client, &self.base_url, &self.username, self.password.as_deref(), "SELECT 1")
            .await
            .map_err(|e| CatalystError::ConnectionLost { reason: e })?;
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
// HTTP helpers
// ---------------------------------------------------------------------------

fn build_base_url(config: &ClickhouseConfig) -> String {
    let scheme = if config.tls.mode != TlsMode::Disabled {
        "https"
    } else {
        "http"
    };
    format!("{}://{}:{}", scheme, config.host, config.port)
}

fn build_client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        .use_rustls_tls()
        .build()
        .map_err(|e| CatalystError::connection_failed(DatabaseType::Clickhouse, "unknown", e.to_string()))
}

async fn send_raw(
    client: &Client,
    base_url: &str,
    username: &str,
    password: Option<&str>,
    sql: &str,
) -> std::result::Result<String, String> {
    let url = format!("{base_url}/");
    let mut req = client
        .post(&url)
        .header("X-ClickHouse-User", username)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(sql.to_owned());

    if let Some(pw) = password {
        req = req.header("X-ClickHouse-Key", pw);
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;

    if status != StatusCode::OK {
        return Err(body.trim().to_string());
    }

    Ok(body)
}

async fn fetch_scalar(
    client: &Client,
    base_url: &str,
    username: &str,
    password: Option<&str>,
    sql: &str,
) -> std::result::Result<String, String> {
    let full = format!("{sql} FORMAT TabSeparated");
    let body = send_raw(client, base_url, username, password, &full).await?;
    Ok(body.trim().to_string())
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChResponse {
    meta: Vec<ChMeta>,
    data: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    statistics: Option<ChStats>,
}

#[derive(Deserialize)]
struct ChMeta {
    name: String,
    #[serde(rename = "type")]
    col_type: String,
}

#[derive(Deserialize)]
struct ChStats {
    elapsed: f64,
}

fn parse_response(raw: &str, fallback_ms: u64) -> Result<QueryResult> {
    // DDL and INSERT responses are empty or just "Ok."
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "Ok." {
        return Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            duration_ms: fallback_ms,
            explain_plan: None,
        });
    }

    let resp: ChResponse = serde_json::from_str(raw)
        .map_err(|e| CatalystError::query_failed(format!("Failed to parse ClickHouse response: {e}")))?;

    let duration_ms = resp
        .statistics
        .as_ref()
        .map(|s| (s.elapsed * 1000.0) as u64)
        .unwrap_or(fallback_ms);

    let columns: Vec<Column> = resp
        .meta
        .iter()
        .map(|m| Column {
            name: m.name.clone(),
            col_type: ch_type_to_column_type(&m.col_type),
            nullable: m.col_type.starts_with("Nullable("),
            native_type: m.col_type.clone(),
        })
        .collect();

    let rows: Vec<Row> = resp
        .data
        .into_iter()
        .map(|cells| Row {
            values: cells
                .into_iter()
                .zip(resp.meta.iter())
                .map(|(cell, meta)| json_to_value(cell, &meta.col_type))
                .collect(),
        })
        .collect();

    Ok(QueryResult {
        columns,
        rows,
        rows_affected: None,
        duration_ms,
        explain_plan: None,
    })
}

fn ch_type_to_column_type(t: &str) -> ColumnType {
    let inner = t
        .strip_prefix("Nullable(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(t);

    if inner.starts_with("UInt") || inner.starts_with("Int") {
        ColumnType::Integer
    } else if inner.starts_with("Float") {
        ColumnType::Float
    } else if inner.starts_with("Decimal") {
        ColumnType::Decimal
    } else if inner.starts_with("DateTime") {
        ColumnType::Timestamp
    } else if inner == "Date" || inner == "Date32" {
        ColumnType::Date
    } else if inner == "UUID" {
        ColumnType::Uuid
    } else if inner.starts_with("Array") || inner.starts_with("Tuple") {
        ColumnType::Array
    } else if inner.starts_with("Map") {
        ColumnType::Json
    } else if inner == "Bool" {
        ColumnType::Boolean
    } else {
        ColumnType::Text
    }
}

fn json_to_value(v: serde_json::Value, col_type: &str) -> Value {
    let inner = col_type
        .strip_prefix("Nullable(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(col_type);

    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if inner.starts_with("Float") {
                Value::Float(n.as_f64().unwrap_or(0.0))
            } else if inner.starts_with("Decimal") {
                Value::Decimal(n.to_string())
            } else {
                Value::Int(n.as_i64().unwrap_or(0))
            }
        }
        serde_json::Value::String(s) => {
            if inner.starts_with("DateTime") {
                // ClickHouse returns DateTime as "2024-01-01 12:00:00"
                Value::Timestamp(
                    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                        .map(|dt| chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                )
            } else if inner == "Date" || inner == "Date32" {
                Value::Date(s)
            } else if inner == "UUID" {
                s.parse::<uuid::Uuid>()
                    .map(Value::Uuid)
                    .unwrap_or(Value::Text(s))
            } else if inner.starts_with("Decimal") {
                Value::Decimal(s)
            } else if inner.starts_with("Int") || inner.starts_with("UInt") {
                // Large integers come as strings in JSONCompact
                Value::Int(s.parse().unwrap_or(0))
            } else {
                Value::Text(s)
            }
        }
        serde_json::Value::Array(arr) => {
            Value::Array(arr.into_iter().map(|v| json_to_value(v, "String")).collect())
        }
        serde_json::Value::Object(map) => Value::Object(map),
    }
}
