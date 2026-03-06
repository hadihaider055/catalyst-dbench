//! Tauri commands for connection management.

use dbench_core::types::{ConnectionInfo, DatabaseType};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

/// Input payload for adding/testing a connection.
#[derive(Debug, Deserialize)]
pub struct ConnectionPayload {
    pub name: String,
    pub db_type: DatabaseType,
    pub host: String,
    pub port: Option<u16>,
    pub database: String,
    pub username: String,
    /// Note: password is retrieved from OS keychain by connection_id, not passed here.
    pub connection_id: Option<String>,
    pub tls_enabled: bool,
    pub ssh_tunnel: Option<SshTunnelPayload>,
    pub read_only: bool,
}

#[derive(Debug, Deserialize)]
pub struct SshTunnelPayload {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
}

/// Response returned for all connection operations.
#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub connection_id: Uuid,
    pub info: ConnectionInfo,
}

/// List all currently active connections.
#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionInfo>, String> {
    Ok(state.registry.list())
}

/// Add and open a new connection.
#[tauri::command]
pub async fn add_connection(
    state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<ConnectionResponse, String> {
    tracing::info!(
        db_type = ?payload.db_type,
        host = %payload.host,
        database = %payload.database,
        "Adding connection"
    );

    // TODO: Build the correct Config type from payload, connect via driver.
    // Dispatch on payload.db_type:
    // match payload.db_type {
    //     DatabaseType::Postgres => { let cfg = PostgresConfig { ... }; driver.connect(&cfg).await? }
    //     DatabaseType::Sqlite => { ... }
    //     ...
    // }

    Err("Not yet implemented — driver wiring in progress".into())
}

/// Test a connection without keeping it open.
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<ConnectionInfo, String> {
    // TODO: Build config, call driver.test_connection().
    Err("Not yet implemented".into())
}

/// Open a previously saved connection.
#[tauri::command]
pub async fn open_connection(
    state: State<'_, AppState>,
    connection_id: Uuid,
) -> Result<ConnectionInfo, String> {
    // TODO: Load config from storage, connect.
    Err("Not yet implemented".into())
}

/// Close an active connection.
#[tauri::command]
pub async fn close_connection(
    state: State<'_, AppState>,
    connection_id: Uuid,
) -> Result<(), String> {
    state
        .registry
        .remove(connection_id)
        .map_err(|e| e.to_string())
}

/// Remove a saved connection configuration.
#[tauri::command]
pub async fn remove_connection(
    state: State<'_, AppState>,
    connection_id: Uuid,
) -> Result<(), String> {
    // TODO: Delete from storage + keychain.
    let _ = state.registry.remove(connection_id);
    Ok(())
}
