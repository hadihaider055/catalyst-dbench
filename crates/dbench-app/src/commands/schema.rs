//! Tauri commands for schema introspection.

use dbench_core::schema::DatabaseSchema;
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

/// Get the full schema for a connection (tables, views, indexes, etc.).
#[tauri::command]
pub async fn get_schema(
    state: State<'_, AppState>,
    connection_id: Uuid,
) -> Result<DatabaseSchema, String> {
    state
        .executor
        .inspect_schema(connection_id)
        .await
        .map_err(|e| e.to_string())
}
