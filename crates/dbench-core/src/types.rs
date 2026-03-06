//! Shared primitive types used across all Catalyst DBench crates.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// All supported database types.
///
/// Adding support for a new database requires:
/// 1. Adding a variant here
/// 2. Implementing [`Driver`](crate::Driver) and [`Connection`](crate::Connection)
/// 3. Registering in `dbench-engine/src/registry.rs`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DatabaseType {
    // -- Relational (SQL) --
    /// PostgreSQL (and compatible: Supabase, Neon, etc.)
    Postgres,
    /// MySQL (and MariaDB)
    Mysql,
    /// SQLite (embedded)
    Sqlite,
    /// CockroachDB (Postgres-compatible distributed SQL)
    Cockroachdb,
    /// Microsoft SQL Server
    Mssql,
    /// Oracle Database
    Oracle,
    /// ClickHouse (OLAP)
    Clickhouse,

    // -- Document --
    /// MongoDB
    Mongodb,
    /// FaunaDB
    Fauna,
    /// CouchDB
    Couchdb,

    // -- Key-Value --
    /// Redis / Valkey / Dragonfly
    Redis,
    /// Amazon DynamoDB
    Dynamodb,
    /// etcd
    Etcd,

    // -- Wide-Column --
    /// Apache Cassandra / ScyllaDB
    Cassandra,
    /// Apache HBase
    Hbase,

    // -- Multi-model / Other --
    /// SurrealDB
    Surrealdb,
    /// Elasticsearch / OpenSearch
    Elasticsearch,
    /// InfluxDB (time-series)
    Influxdb,
    /// TigerBeetle (financial ledger)
    Tigerbeetle,
}

impl DatabaseType {
    /// Human-readable display name.
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Postgres => "PostgreSQL",
            Self::Mysql => "MySQL / MariaDB",
            Self::Sqlite => "SQLite",
            Self::Cockroachdb => "CockroachDB",
            Self::Mssql => "Microsoft SQL Server",
            Self::Oracle => "Oracle Database",
            Self::Clickhouse => "ClickHouse",
            Self::Mongodb => "MongoDB",
            Self::Fauna => "FaunaDB",
            Self::Couchdb => "CouchDB",
            Self::Redis => "Redis",
            Self::Dynamodb => "Amazon DynamoDB",
            Self::Etcd => "etcd",
            Self::Cassandra => "Cassandra / ScyllaDB",
            Self::Hbase => "Apache HBase",
            Self::Surrealdb => "SurrealDB",
            Self::Elasticsearch => "Elasticsearch",
            Self::Influxdb => "InfluxDB",
            Self::Tigerbeetle => "TigerBeetle",
        }
    }

    /// Whether this database uses SQL.
    #[must_use]
    pub fn is_sql(&self) -> bool {
        matches!(
            self,
            Self::Postgres
                | Self::Mysql
                | Self::Sqlite
                | Self::Cockroachdb
                | Self::Mssql
                | Self::Oracle
                | Self::Clickhouse
        )
    }

    /// Default port for this database type, if applicable.
    #[must_use]
    pub fn default_port(&self) -> Option<u16> {
        match self {
            Self::Postgres | Self::Cockroachdb => Some(5432),
            Self::Mysql => Some(3306),
            Self::Mssql => Some(1433),
            Self::Oracle => Some(1521),
            Self::Clickhouse => Some(8123),
            Self::Mongodb => Some(27017),
            Self::Redis => Some(6379),
            Self::Cassandra => Some(9042),
            Self::Elasticsearch => Some(9200),
            Self::Influxdb => Some(8086),
            Self::Etcd => Some(2379),
            _ => None,
        }
    }
}

/// Metadata about an active connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Unique identifier for this connection instance.
    pub id: Uuid,
    /// The database type.
    pub db_type: DatabaseType,
    /// Server hostname or IP.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Database/schema name.
    pub database: String,
    /// Username used to connect.
    pub username: String,
    /// Whether TLS is active on this connection.
    pub tls_active: bool,
    /// Whether an SSH tunnel is used.
    pub ssh_tunnel: bool,
    /// Server version string (populated after connection).
    pub server_version: Option<String>,
    /// When this connection was opened.
    pub connected_at: DateTime<Utc>,
}

/// Connection mode: controls read/write permissions enforced at the driver level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    /// Full read-write access.
    #[default]
    ReadWrite,
    /// Read-only: SELECT/GET only. Write operations are rejected before reaching the DB.
    ReadOnly,
    /// Read-only AND data export is disabled.
    ReadOnlyNoExport,
}

impl ConnectionMode {
    /// Returns `true` if write operations are permitted.
    #[must_use]
    pub fn allows_writes(&self) -> bool {
        matches!(self, Self::ReadWrite)
    }

    /// Returns `true` if data export is permitted.
    #[must_use]
    pub fn allows_export(&self) -> bool {
        !matches!(self, Self::ReadOnlyNoExport)
    }
}
