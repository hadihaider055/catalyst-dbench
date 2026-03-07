//! Redis driver implementation.
//!
//! Uses `redis-rs` with async Tokio runtime support.
//! Compatible with: Redis 6+, Valkey, Dragonfly, Upstash.

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::Query,
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::{DatabaseSchema, KeyPatternSchema, SchemaObject},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use redis::{
    aio::ConnectionManager,
    Client, RedisResult, Value as RedisValue,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for connecting to Redis.
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis hostname.
    #[config(required)]
    pub host: String,
    /// Redis port (default: 6379).
    pub port: u16,
    /// Redis database index (0–15, default: 0).
    pub db_index: u8,
    /// Password (ACL or `requirepass`), stored in OS keychain.
    #[config(secret)]
    pub password: Option<String>,
    /// Username (Redis 6+ ACL).
    pub username: Option<String>,
    /// Use TLS (for Redis TLS / Upstash / ElastiCache TLS).
    pub tls: bool,
    /// Connection mode.
    pub mode: ConnectionMode,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 6379,
            db_index: 0,
            password: None,
            username: None,
            tls: false,
            mode: ConnectionMode::ReadWrite,
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// Redis driver.
#[derive(Debug, Default)]
pub struct RedisDriver;

impl Driver for RedisDriver {
    type Connection = RedisConnection;
    type Config = RedisConfig;

    fn name(&self) -> &'static str { "redis" }
    fn database_type(&self) -> DatabaseType { DatabaseType::Redis }
    fn default_port(&self) -> Option<u16> { Some(6379) }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        let scheme = if config.tls { "rediss" } else { "redis" };

        // Build URL: redis://[user:pass@]host:port/db_index
        let url = match (&config.username, &config.password) {
            (Some(user), Some(pass)) => {
                format!("{}://{}:{}@{}:{}/{}", scheme, user, pass, config.host, config.port, config.db_index)
            }
            (None, Some(pass)) => {
                format!("{}://:{}@{}:{}/{}", scheme, pass, config.host, config.port, config.db_index)
            }
            _ => format!("{}://{}:{}/{}", scheme, config.host, config.port, config.db_index),
        };

        tracing::info!(host = %config.host, port = config.port, db = config.db_index, "Connecting to Redis");

        let client = Client::open(url.as_str())
            .map_err(|e| CatalystError::connection_failed(DatabaseType::Redis, &config.host, e.to_string()))?;

        let manager = ConnectionManager::new(client)
            .await
            .map_err(|e| CatalystError::connection_failed(DatabaseType::Redis, &config.host, e.to_string()))?;

        // Fetch server version via INFO server.
        let server_version = {
            let mut mgr = manager.clone();
            let info: RedisResult<String> = redis::cmd("INFO").arg("server").query_async(&mut mgr).await;
            info.ok().and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("redis_version:"))
                    .map(|l| l.trim_start_matches("redis_version:").trim().to_string())
            })
        };

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Redis,
            host: config.host.clone(),
            port: config.port,
            database: config.db_index.to_string(),
            username: config.username.clone().unwrap_or_default(),
            tls_active: config.tls,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        tracing::info!(conn_id = %info.id, "Redis connection established");

        Ok(RedisConnection {
            info,
            mode: config.mode,
            alive: true,
            manager,
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// An active Redis connection.
pub struct RedisConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    manager: ConnectionManager,
}

/// Read-only commands — everything else is a potential write.
const READ_COMMANDS: &[&str] = &[
    "GET", "MGET", "GETRANGE", "STRLEN",
    "HGET", "HMGET", "HGETALL", "HKEYS", "HVALS", "HLEN", "HEXISTS",
    "LRANGE", "LINDEX", "LLEN",
    "SMEMBERS", "SCARD", "SISMEMBER", "SMISMEMBER", "SDIFF", "SINTER", "SUNION",
    "ZRANGE", "ZRANGEBYSCORE", "ZREVRANGE", "ZCARD", "ZSCORE", "ZRANK", "ZCOUNT",
    "EXISTS", "TYPE", "TTL", "PTTL", "KEYS", "SCAN", "HSCAN", "SSCAN", "ZSCAN",
    "INFO", "PING", "DBSIZE", "TIME", "COMMAND", "CLIENT",
    "OBJECT", "DEBUG",
];

impl Connection for RedisConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost { reason: "connection is closed".into() });
        }

        // Parse "COMMAND arg1 arg2 ..." from query text.
        let text = query.text.trim();
        let mut parts = shell_split(text);
        if parts.is_empty() {
            return Err(CatalystError::query_failed("Empty Redis command"));
        }

        let cmd_name = parts[0].to_uppercase();

        // Read-only guard.
        if !self.mode.allows_writes() && !READ_COMMANDS.contains(&cmd_name.as_str()) {
            return Err(CatalystError::ReadOnlyViolation);
        }

        let start = Instant::now();

        let mut cmd = redis::cmd(&cmd_name);
        for arg in parts.drain(1..) {
            cmd.arg(arg);
        }

        let result: RedisResult<RedisValue> = cmd.query_async(&mut self.manager).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(val) => Ok(redis_value_to_result(&cmd_name, val, duration_ms)),
            Err(e) => Err(CatalystError::query_failed(e.to_string())),
        }
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        // Scan keys and group by prefix pattern (everything before the first ':').
        let mut pattern_counts: std::collections::HashMap<String, (String, u64)> = std::collections::HashMap::new();
        let mut cursor: u64 = 0;
        let mut total_scanned = 0u64;
        const MAX_SCAN: u64 = 5000;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("COUNT")
                .arg(200u64)
                .query_async(&mut self.manager)
                .await
                .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

            for key in &keys {
                total_scanned += 1;
                // Detect key type for the first occurrence of each pattern.
                let pattern = key.split(':').next().unwrap_or(key).to_string();
                let entry = pattern_counts.entry(pattern).or_insert(("string".into(), 0));
                entry.1 += 1;
                if entry.1 == 1 {
                    // Fetch type for the first key in this group.
                    let key_type: RedisResult<String> = redis::cmd("TYPE")
                        .arg(key)
                        .query_async(&mut self.manager)
                        .await;
                    if let Ok(t) = key_type {
                        entry.0 = t;
                    }
                }
            }

            cursor = next_cursor;
            if cursor == 0 || total_scanned >= MAX_SCAN {
                break;
            }
        }

        let objects: Vec<SchemaObject> = pattern_counts
            .into_iter()
            .map(|(pattern, (key_type, sample_count))| {
                SchemaObject::KeyPattern(KeyPatternSchema {
                    pattern: format!("{pattern}:*"),
                    key_type,
                    sample_count,
                })
            })
            .collect();

        Ok(DatabaseSchema {
            name: format!("redis-db{}", self.info.database),
            db_type: DatabaseType::Redis,
            server_version: self.info.server_version.clone().unwrap_or_else(|| "Redis".into()),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        let _: RedisResult<String> = redis::cmd("PING").query_async(&mut self.manager).await;
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

/// Simple shell-style argument splitter (handles quoted strings).
fn shell_split(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for ch in input.chars() {
        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quotes = true;
            quote_char = ch;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                args.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

fn redis_value_to_value(val: RedisValue) -> Value {
    match val {
        RedisValue::Nil => Value::Null,
        RedisValue::Int(i) => Value::Int(i),
        RedisValue::Data(bytes) => {
            String::from_utf8(bytes.clone())
                .map(Value::Text)
                .unwrap_or_else(|_| Value::Bytes(bytes))
        }
        RedisValue::Status(s) => Value::Text(s),
        RedisValue::Okay => Value::Text("OK".into()),
        RedisValue::Bulk(arr) => {
            Value::Array(arr.into_iter().map(redis_value_to_value).collect())
        }
    }
}

fn redis_value_to_result(cmd: &str, val: RedisValue, duration_ms: u64) -> QueryResult {
    match val {
        RedisValue::Nil => QueryResult {
            columns: vec![Column { name: "result".into(), col_type: ColumnType::Text, nullable: true, native_type: "nil".into() }],
            rows: vec![Row { values: vec![Value::Null] }],
            rows_affected: None,
            duration_ms,
            explain_plan: None,
        },
        RedisValue::Bulk(ref items) if !items.is_empty() => {
            // HGETALL returns alternating key/value pairs.
            if cmd == "HGETALL" && items.len() % 2 == 0 {
                let keys: Vec<String> = items.iter().step_by(2).map(|v: &RedisValue| {
                    match v {
                        RedisValue::Data(b) => String::from_utf8_lossy(b).into_owned(),
                        RedisValue::Status(s) => s.clone(),
                        _ => format!("{v:?}"),
                    }
                }).collect();
                let columns: Vec<Column> = keys.iter().map(|k: &String| Column {
                    name: k.clone(),
                    col_type: ColumnType::Text,
                    nullable: true,
                    native_type: "string".into(),
                }).collect();
                let values: Vec<Value> = items.iter().skip(1).step_by(2).map(|v: &RedisValue| {
                    redis_value_to_value(v.clone())
                }).collect();
                return QueryResult {
                    columns,
                    rows: vec![Row { values }],
                    rows_affected: None,
                    duration_ms,
                    explain_plan: None,
                };
            }
            // Generic array — one row per element.
            let columns = vec![Column { name: "value".into(), col_type: ColumnType::Text, nullable: true, native_type: "string".into() }];
            let rows: Vec<Row> = items.iter().map(|item: &RedisValue| Row {
                values: vec![redis_value_to_value(item.clone())],
            }).collect();
            QueryResult { columns, rows, rows_affected: None, duration_ms, explain_plan: None }
        }
        other => {
            let value = redis_value_to_value(other);
            let rows_affected = if cmd == "DEL" || cmd == "SET" || cmd == "HSET" {
                match &value { Value::Int(n) => Some(*n as u64), _ => None }
            } else {
                None
            };
            QueryResult {
                columns: vec![Column { name: "result".into(), col_type: ColumnType::Text, nullable: true, native_type: "string".into() }],
                rows: vec![Row { values: vec![value] }],
                rows_affected,
                duration_ms,
                explain_plan: None,
            }
        }
    }
}
