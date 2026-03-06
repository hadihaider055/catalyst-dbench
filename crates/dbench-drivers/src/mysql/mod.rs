//! MySQL / MariaDB driver (stub — full implementation uses sqlx).

use std::time::{Duration, Instant};

use dbench_macros::ConnectionConfig;
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
use dbench_security::tls::{TlsConfig, TlsMode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
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
        }
    }
}

#[derive(Debug, Default)]
pub struct MysqlDriver;

impl Driver for MysqlDriver {
    type Connection = MysqlConnection;
    type Config = MysqlConfig;

    fn name(&self) -> &'static str { "mysql" }
    fn database_type(&self) -> DatabaseType { DatabaseType::Mysql }
    fn default_port(&self) -> Option<u16> { Some(3306) }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        tracing::info!(host = %config.host, port = config.port, "Connecting to MySQL");
        // TODO: Use sqlx::MySqlPool::connect() for full implementation.
        Ok(MysqlConnection {
            info: ConnectionInfo {
                id: Uuid::new_v4(),
                db_type: DatabaseType::Mysql,
                host: config.host.clone(),
                port: config.port,
                database: config.database.clone(),
                username: config.username.clone(),
                tls_active: config.tls.mode != TlsMode::Disabled,
                ssh_tunnel: false,
                server_version: None,
                connected_at: chrono::Utc::now(),
            },
            mode: config.mode,
            alive: true,
        })
    }
}

pub struct MysqlConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
}

impl Connection for MysqlConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost { reason: "closed".into() });
        }
        if !self.mode.allows_writes() {
            let up = query.text.trim_start().to_uppercase();
            if ["INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER"]
                .iter().any(|k| up.starts_with(k)) {
                return Err(CatalystError::ReadOnlyViolation);
            }
        }
        let start = Instant::now();
        Ok(QueryResult {
            columns: vec![], rows: vec![], rows_affected: Some(0),
            duration_ms: start.elapsed().as_millis() as u64,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        Ok(DatabaseSchema {
            name: self.info.database.clone(),
            db_type: DatabaseType::Mysql,
            server_version: "MySQL".into(),
            objects: vec![],
        })
    }

    async fn ping(&mut self) -> Result<Duration> { Ok(Duration::ZERO) }
    async fn close(mut self) -> Result<()> { self.alive = false; Ok(()) }
    fn is_alive(&self) -> bool { self.alive }
    fn info(&self) -> &ConnectionInfo { &self.info }
    fn mode(&self) -> ConnectionMode { self.mode }
}
