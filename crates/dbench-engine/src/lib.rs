//! # dbench-engine
//!
//! The runtime engine for Catalyst DBench. Manages connections, routes queries,
//! enforces read-only guards, and emits audit events.

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]

pub mod executor;
pub mod registry;

pub use executor::QueryExecutor;
pub use registry::{BoxConnection, ConnectionAdapter, ConnectionRegistry, DynConnection};
