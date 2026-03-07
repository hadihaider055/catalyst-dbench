//! Connection registry — tracks all open connections by UUID.
//!
//! `DynConnection` is the object-safe async interface stored in the registry.
//! `ConnectionAdapter<C>` wraps any concrete `Connection` impl into a `DynConnection`.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use dbench_core::{
    connection::Connection,
    query::Query,
    result::QueryResult,
    schema::DatabaseSchema,
    types::ConnectionInfo,
    CatalystError, Result,
};
use dashmap::DashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DynConnection — object-safe async trait
// ---------------------------------------------------------------------------

/// Object-safe async connection interface (via `async_trait`).
#[async_trait]
pub trait DynConnection: Send + Sync {
    /// Return metadata about this connection.
    fn info(&self) -> &ConnectionInfo;
    /// Check if the connection is believed to be alive (no network call).
    fn is_alive(&self) -> bool;
    /// Execute a query and return results.
    async fn execute(&mut self, query: &Query) -> Result<QueryResult>;
    /// Inspect the database schema.
    async fn inspect_schema(&mut self) -> Result<DatabaseSchema>;
    /// Ping the server and return round-trip duration.
    async fn ping(&mut self) -> Result<Duration>;
}

/// Type alias for a heap-allocated, type-erased connection.
pub type BoxConnection = Box<dyn DynConnection>;

// ---------------------------------------------------------------------------
// ConnectionAdapter — bridges Connection → DynConnection
// ---------------------------------------------------------------------------

/// Wraps any `Connection` impl so it can be stored as `BoxConnection`.
///
/// This adapter bridges the AFIT-based `Connection` trait to the
/// `async_trait`-based `DynConnection` for type-erased storage.
pub struct ConnectionAdapter<C>(pub C);

#[async_trait]
impl<C> DynConnection for ConnectionAdapter<C>
where
    C: Connection + Send + Sync + 'static,
{
    fn info(&self) -> &ConnectionInfo { self.0.info() }
    fn is_alive(&self) -> bool { self.0.is_alive() }

    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        self.0.execute(query).await
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        self.0.inspect_schema().await
    }

    async fn ping(&mut self) -> Result<Duration> {
        self.0.ping().await
    }
}

// ---------------------------------------------------------------------------
// ConnectionRegistry
// ---------------------------------------------------------------------------

/// Thread-safe registry of all active connections.
///
/// Shared via `Arc<ConnectionRegistry>` between `AppState` and `QueryExecutor`.
#[derive(Default)]
pub struct ConnectionRegistry {
    connections: DashMap<Uuid, Arc<tokio::sync::Mutex<BoxConnection>>>,
    infos: DashMap<Uuid, ConnectionInfo>,
}

impl ConnectionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap and register a concrete connection. Returns the connection UUID.
    pub fn register<C>(&self, conn: C) -> Uuid
    where
        C: Connection + Send + Sync + 'static,
    {
        let id = conn.info().id;
        let info = conn.info().clone();
        let boxed: BoxConnection = Box::new(ConnectionAdapter(conn));
        self.connections
            .insert(id, Arc::new(tokio::sync::Mutex::new(boxed)));
        self.infos.insert(id, info);
        tracing::debug!(conn_id = %id, "Connection registered");
        id
    }

    /// Remove a connection by ID.
    pub fn remove(&self, id: Uuid) -> Result<()> {
        self.connections
            .remove(&id)
            .ok_or_else(|| CatalystError::ConnectionLost {
                reason: format!("connection {id} not found"),
            })?;
        self.infos.remove(&id);
        Ok(())
    }

    /// Retrieve a connection handle by ID for exclusive access.
    pub fn get(&self, id: Uuid) -> Option<Arc<tokio::sync::Mutex<BoxConnection>>> {
        self.connections.get(&id).map(|v| Arc::clone(v.value()))
    }

    /// List metadata for all active connections.
    #[must_use]
    pub fn list(&self) -> Vec<ConnectionInfo> {
        self.infos.iter().map(|e| e.value().clone()).collect()
    }

    /// Return the number of active connections.
    #[must_use]
    pub fn len(&self) -> usize { self.connections.len() }

    /// Return `true` if there are no active connections.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.connections.is_empty() }
}
