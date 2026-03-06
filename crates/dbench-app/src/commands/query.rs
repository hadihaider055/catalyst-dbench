//! Tauri commands for query execution.

use dbench_core::{query::Query, result::QueryResult};
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct QueryPayload {
    pub connection_id: String,
    pub sql: String,
    pub params: Option<Vec<serde_json::Value>>,
    pub timeout_ms: Option<u64>,
    pub explain: Option<bool>,
}

/// Execute a single query on a connection.
#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    payload: QueryPayload,
) -> Result<QueryResult, String> {
    if payload.sql.trim().is_empty() {
        return Err("Query cannot be empty".into());
    }
    if payload.sql.len() > 1_000_000 {
        return Err("Query exceeds maximum length of 1MB".into());
    }

    let conn_id = Uuid::parse_str(&payload.connection_id).map_err(|e| e.to_string())?;

    let mut query = Query::new(payload.sql);
    if let Some(ms) = payload.timeout_ms {
        query = query.with_timeout(ms);
    }
    if payload.explain.unwrap_or(false) {
        query = query.explain();
    }

    state
        .executor
        .execute(conn_id, query)
        .await
        .map_err(|e| e.to_string())
}

/// Execute multiple queries in sequence.
#[tauri::command]
pub async fn execute_batch(
    state: State<'_, AppState>,
    connection_id: String,
    queries: Vec<String>,
) -> Result<Vec<QueryResult>, String> {
    if queries.is_empty() {
        return Ok(vec![]);
    }
    if queries.len() > 100 {
        return Err("Batch size cannot exceed 100 queries".into());
    }

    let conn_id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;

    let mut results = Vec::with_capacity(queries.len());
    for sql in queries {
        let result = state
            .executor
            .execute(conn_id, Query::new(sql))
            .await
            .map_err(|e| e.to_string())?;
        results.push(result);
    }
    Ok(results)
}
