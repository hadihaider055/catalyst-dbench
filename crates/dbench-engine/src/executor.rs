//! Query executor — routes queries to connections with audit hooks.

use std::sync::Arc;

use dbench_core::{
    error::CatalystError,
    query::Query,
    result::QueryResult,
    schema::DatabaseSchema,
    Result,
};
use dbench_security::{
    audit::{AuditEvent, AuditLogger},
    crypto::sha256_hex,
};
use uuid::Uuid;

use crate::registry::ConnectionRegistry;

/// Routes queries to connections, enforces guards, and emits audit events.
pub struct QueryExecutor {
    registry: Arc<ConnectionRegistry>,
    audit: AuditLogger,
}

impl QueryExecutor {
    #[must_use]
    pub fn new(registry: Arc<ConnectionRegistry>, audit: AuditLogger) -> Self {
        Self { registry, audit }
    }

    /// Execute a query on the connection identified by `conn_id`.
    pub async fn execute(&self, conn_id: Uuid, query: Query) -> Result<QueryResult> {
        let conn_handle = self.registry.get(conn_id).ok_or_else(|| {
            CatalystError::ConnectionLost {
                reason: format!("connection {conn_id} not found"),
            }
        })?;

        let start = std::time::Instant::now();
        let query_hash = sha256_hex(query.text.as_bytes());

        let mut conn = conn_handle.lock().await;
        let result = conn.execute(&query).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match &result {
            Ok(r) => {
                let _ = self
                    .audit
                    .log(AuditEvent::QueryExecute {
                        conn_id: conn_id.to_string(),
                        query_hash: format!("sha256:{query_hash}"),
                        rows_returned: Some(r.rows.len() as u64),
                        rows_affected: r.rows_affected,
                        duration_ms,
                    })
                    .await;
            }
            Err(e) => {
                let _ = self
                    .audit
                    .log(AuditEvent::QueryFailed {
                        conn_id: conn_id.to_string(),
                        query_hash: format!("sha256:{query_hash}"),
                        error: e.to_string(),
                        duration_ms,
                    })
                    .await;
            }
        }

        result
    }

    /// Inspect the schema for a connection.
    pub async fn inspect_schema(&self, conn_id: Uuid) -> Result<DatabaseSchema> {
        let conn_handle = self.registry.get(conn_id).ok_or_else(|| {
            CatalystError::ConnectionLost {
                reason: format!("connection {conn_id} not found"),
            }
        })?;

        let mut conn = conn_handle.lock().await;
        let schema = conn.inspect_schema().await?;

        let _ = self
            .audit
            .log(AuditEvent::SchemaInspected {
                conn_id: conn_id.to_string(),
                object_count: schema.objects.len() as u64,
            })
            .await;

        Ok(schema)
    }
}
