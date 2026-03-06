//! Connection registry — tracks all open connections by UUID.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use dbench_core::{query::Query, result::QueryResult, schema::DatabaseSchema, types::ConnectionInfo, CatalystError, Result};
use dashmap::DashMap;
use uuid::Uuid;

/// Object-safe async connection interface (dyn-safe via async_trait).
#[async_trait]
pub trait DynConnection: Send + Sync {
    fn info(&self) -> &ConnectionInfo;
    fn is_alive(&self) -> bool;
    async fn execute(&mut self, query: &Query) -> Result<QueryResult>;
    async fn inspect_schema(&mut self) -> Result<DatabaseSchema>;
    async fn ping(&mut self) -> Result<Duration>;
}

pub type BoxConnection = Box<dyn DynConnection>;

/// Thread-safe registry of all active connections.
pub struct ConnectionRegistry {
    connections: Arc<DashMap<Uuid, Arc<tokio::sync::Mutex<BoxConnection>>>>,
    infos: Arc<DashMap<Uuid, ConnectionInfo>>,
}

impl ConnectionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            infos: Arc::new(DashMap::new()),
        }
    }

    /// Register a connection, returns its UUID.
    pub fn register(&self, conn: BoxConnection) -> Uuid {
        let id = conn.info().id;
        let info = conn.info().clone();
        self.connections.insert(id, Arc::new(tokio::sync::Mutex::new(conn)));
        self.infos.insert(id, info);
        tracing::debug!(conn_id = %id, "Connection registered");
        id
    }

    /// Remove a connection by ID.
    pub fn remove(&self, id: Uuid) -> Result<()> {
        self.connections.remove(&id).ok_or_else(|| CatalystError::ConnectionLost {
            reason: format!("connection {id} not found"),
        })?;
        self.infos.remove(&id);
        Ok(())
    }

    pub fn get(&self, id: Uuid) -> Option<Arc<tokio::sync::Mutex<BoxConnection>>> {
        self.connections.get(&id).map(|v| Arc::clone(v.value()))
    }

    #[must_use]
    pub fn list(&self) -> Vec<ConnectionInfo> {
        self.infos.iter().map(|e| e.value().clone()).collect()
    }

    #[must_use]
    pub fn len(&self) -> usize { self.connections.len() }

    #[must_use]
    pub fn is_empty(&self) -> bool { self.connections.is_empty() }
}

impl Default for ConnectionRegistry {
    fn default() -> Self { Self::new() }
}
