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
    result::QueryResult,
    schema::DatabaseSchema,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

/// Redis driver.
#[derive(Debug, Default)]
pub struct RedisDriver;

impl Driver for RedisDriver {
    type Connection = RedisConnection;
    type Config = RedisConfig;

    fn name(&self) -> &'static str {
        "redis"
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::Redis
    }

    fn default_port(&self) -> Option<u16> {
        Some(6379)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        let scheme = if config.tls { "rediss" } else { "redis" };
        let url = format!("{}://{}:{}/{}", scheme, config.host, config.port, config.db_index);

        tracing::info!(url = %url, "Connecting to Redis");

        // TODO: Build redis::Client:
        // let client = redis::Client::open(url.as_str())
        //     .map_err(|e| CatalystError::connection_failed(DatabaseType::Redis, &config.host, e))?;
        // let manager = redis::aio::ConnectionManager::new(client).await?;

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Redis,
            host: config.host.clone(),
            port: config.port,
            database: config.db_index.to_string(),
            username: config.username.clone().unwrap_or_default(),
            tls_active: config.tls,
            ssh_tunnel: false,
            server_version: None,
            connected_at: chrono::Utc::now(),
        };

        Ok(RedisConnection {
            info,
            mode: config.mode,
            alive: true,
        })
    }
}

/// An active Redis connection.
pub struct RedisConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    // TODO: manager: redis::aio::ConnectionManager,
}

impl Connection for RedisConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }

        // Redis commands are passed as the query text (e.g., "GET mykey", "HGETALL myhash").
        // Write guard:
        if !self.mode.allows_writes() {
            let cmd = query.text.trim_start().to_uppercase();
            let write_cmds = ["SET", "DEL", "HSET", "LPUSH", "RPUSH", "SADD", "ZADD",
                              "EXPIRE", "RENAME", "FLUSHDB", "FLUSHALL"];
            if write_cmds.iter().any(|c| cmd.starts_with(c)) {
                return Err(CatalystError::ReadOnlyViolation);
            }
        }

        let start = Instant::now();
        // TODO: Parse and execute Redis command via redis-rs.
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            duration_ms: start.elapsed().as_millis() as u64,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        // Redis "schema": scan keys, group by pattern, detect type.
        // TODO: SCAN with COUNT 1000, group keys by prefix pattern.
        Ok(DatabaseSchema {
            name: format!("redis-db{}", self.info.database),
            db_type: DatabaseType::Redis,
            server_version: self.info.server_version.clone().unwrap_or("unknown".into()),
            objects: vec![],
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        // TODO: redis::cmd("PING").query_async(&mut self.manager).await?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        self.alive = false;
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
