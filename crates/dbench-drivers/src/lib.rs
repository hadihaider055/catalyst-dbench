//! # dbench-drivers
//!
//! Database driver implementations for Catalyst DBench.
//!
//! Each driver module implements the [`Driver`](dbench_core::Driver) and
//! [`Connection`](dbench_core::Connection) traits from `dbench-core`.
//!
//! ## Feature Flags
//!
//! Each driver is behind a feature flag to avoid pulling in unused dependencies:
//!
//! | Feature | Driver | Default? |
//! |---------|--------|----------|
//! | `postgres` | PostgreSQL | yes |
//! | `sqlite` | SQLite | yes |
//! | `mysql` | MySQL / MariaDB | no |
//! | `mongodb` | MongoDB | no |
//! | `redis` | Redis | no |
//! | `clickhouse` | ClickHouse (HTTP) | no |
//! | `all` | All of the above | no |
//! | `cassandra` | Cassandra / ScyllaDB | no |
//!
//! ## Adding a New Driver
//!
//! 1. Create a module under `src/` (e.g., `src/clickhouse/`)
//! 2. Add a feature flag in `Cargo.toml`
//! 3. Implement [`Driver`] and [`Connection`] traits
//! 4. Add a `ConnectionConfig` struct with `#[derive(ConnectionConfig)]`
//! 5. Re-export from this file
//! 6. Register in `dbench-engine/src/registry.rs`

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "mysql")]
pub mod mysql;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "mongodb")]
pub mod mongodb;

#[cfg(feature = "redis")]
pub mod redis;

#[cfg(feature = "clickhouse")]
pub mod clickhouse;

#[cfg(feature = "cassandra")]
pub mod cassandra;

// Convenience re-exports
#[cfg(feature = "postgres")]
pub use postgres::{PostgresConfig, PostgresDriver};

#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteConfig, SqliteDriver};

#[cfg(feature = "mysql")]
pub use mysql::{MysqlConfig, MysqlDriver};

#[cfg(feature = "mongodb")]
pub use mongodb::{MongoConfig, MongoDriver};

#[cfg(feature = "redis")]
pub use redis::{RedisConfig, RedisDriver};

#[cfg(feature = "clickhouse")]
pub use clickhouse::{ClickhouseConfig, ClickhouseDriver};

#[cfg(feature = "cassandra")]
pub use cassandra::{CassandraConfig, CassandraDriver};
