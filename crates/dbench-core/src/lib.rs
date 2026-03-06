//! # dbench-core
//!
//! The foundational crate for Catalyst DBench. Defines the traits, types, and errors
//! that every database driver must implement.
//!
//! ## Key Abstractions
//!
//! - [`Driver`] — entry point for a database type (implements how to connect)
//! - [`Connection`] — an active, live connection to a database instance
//! - [`Query`] — a parameterized database query
//! - [`QueryResult`] — the result of executing a query
//! - [`DatabaseSchema`] — introspected schema (tables, collections, indexes)
//! - [`DatabaseType`] — enum of all supported database types

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod connection;
pub mod driver;
pub mod error;
pub mod query;
pub mod result;
pub mod schema;
pub mod types;

// Re-export primary types at the crate root.
pub use connection::Connection;
pub use driver::Driver;
pub use error::CatalystError;
pub use query::{Query, QueryParam};
pub use result::{Column, ColumnType, QueryResult, Row, Value};
pub use schema::{
    ColumnSchema, DatabaseSchema, IndexSchema, SchemaObject, TableSchema,
};
pub use types::{ConnectionConfig, ConnectionInfo, ConnectionMode, DatabaseType};

/// Convenience result type for dbench-core operations.
pub type Result<T> = std::result::Result<T, CatalystError>;
