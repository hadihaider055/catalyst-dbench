//! # dbench-engine
//!
//! The runtime engine for Catalyst DBench. Sits between the Tauri app and the drivers.
//!
//! ## Responsibilities
//!
//! - **Connection Registry** — tracks all open/saved connections by ID
//! - **Query Executor** — routes queries to the correct driver, enforces guards
//! - **Schema Inspector** — caches and invalidates schema introspection results
//! - **Read-Only Guard** — enforces `ConnectionMode` before queries hit the driver
//! - **Audit Hook** — emits audit events for every operation

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod executor;
pub mod registry;

pub use executor::QueryExecutor;
pub use registry::ConnectionRegistry;
