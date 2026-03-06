//! The `Connection` trait — a live, active database connection.

use std::time::Duration;

use crate::{
    query::Query,
    result::QueryResult,
    schema::DatabaseSchema,
    types::{ConnectionInfo, ConnectionMode},
    Result,
};

/// A live connection to a database instance.
///
/// Connections are not cheap to create (they involve TCP handshakes, TLS,
/// authentication). The engine layer pools them via `dbench-engine`.
///
/// # Thread Safety
///
/// Connections are `Send + Sync` to support multi-threaded async environments.
/// Implementations that use non-thread-safe underlying clients should wrap
/// them in a `Mutex`.
pub trait Connection: Send + Sync {
    /// Execute a single query and return the result.
    ///
    /// This is the primary method for all database operations. The query
    /// must use parameterized values — never raw string interpolation.
    ///
    /// # Errors
    /// Returns [`CatalystError`] on query error, network error, or timeout.
    fn execute(
        &mut self,
        query: &Query,
    ) -> impl std::future::Future<Output = Result<QueryResult>> + Send;

    /// Execute multiple queries in sequence.
    ///
    /// Implementations may use a transaction or multi-statement protocol
    /// if supported. Results are returned in the same order as the queries.
    ///
    /// # Errors
    /// Stops at the first error and returns it. Completed query results are discarded.
    fn execute_batch(
        &mut self,
        queries: &[Query],
    ) -> impl std::future::Future<Output = Result<Vec<QueryResult>>> + Send {
        async move {
            let mut results = Vec::with_capacity(queries.len());
            for q in queries {
                results.push(self.execute(q).await?);
            }
            Ok(results)
        }
    }

    /// Inspect the database schema (tables, collections, indexes, etc.).
    ///
    /// # Errors
    /// Returns [`CatalystError`] if schema introspection fails or is not supported.
    fn inspect_schema(
        &mut self,
    ) -> impl std::future::Future<Output = Result<DatabaseSchema>> + Send;

    /// Ping the server to check the connection is alive.
    ///
    /// Returns the round-trip latency.
    ///
    /// # Errors
    /// Returns [`CatalystError`] if the ping fails (connection is dead).
    fn ping(&mut self) -> impl std::future::Future<Output = Result<Duration>> + Send;

    /// Close the connection gracefully.
    ///
    /// # Errors
    /// Returns [`CatalystError`] if the close handshake fails (network error).
    /// The connection is considered closed regardless.
    fn close(self) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Check whether the connection is believed to be alive (best-effort, no network call).
    ///
    /// Use [`ping`](Connection::ping) for a definitive check.
    fn is_alive(&self) -> bool;

    /// Return metadata about this connection (type, host, database name, TLS status, etc.).
    fn info(&self) -> &ConnectionInfo;

    /// Return the current connection mode (read-write vs read-only).
    fn mode(&self) -> ConnectionMode {
        ConnectionMode::ReadWrite
    }
}
