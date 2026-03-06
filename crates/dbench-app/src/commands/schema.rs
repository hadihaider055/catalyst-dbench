//! Tauri commands for schema introspection.

use dbench_core::schema::DatabaseSchema;
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

/// Get the full schema for a connection (tables, views, indexes, etc.).
#[tauri::command]
pub async fn get_schema(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<DatabaseSchema, String> {
    let conn_id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;
    state
        .executor
        .inspect_schema(conn_id)
        .await
        .map_err(|e| e.to_string())
}
