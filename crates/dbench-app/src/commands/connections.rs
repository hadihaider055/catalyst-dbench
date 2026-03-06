//! Tauri commands for connection management.

use dbench_core::types::{ConnectionInfo, ConnectionMode, DatabaseType};
use dbench_drivers::{
    postgres::{PostgresConfig, PostgresDriver},
    sqlite::{SqliteConfig, SqliteDriver},
};
use dbench_security::tls::TlsConfig;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------

/// Input payload for adding or testing a connection.
#[derive(Debug, Deserialize)]
pub struct ConnectionPayload {
    pub name: String,
    pub db_type: DatabaseType,
    pub host: String,
    pub port: Option<u16>,
    pub database: String,
    pub username: String,
    pub password: Option<String>,
    pub tls_enabled: bool,
    pub read_only: bool,
}

#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub connection_id: String,
    pub info: ConnectionInfo,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// List all currently active connections.
#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionInfo>, String> {
    Ok(state.registry.list())
}

/// Open and register a new connection.
#[tauri::command]
pub async fn add_connection(
    state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<ConnectionResponse, String> {
    tracing::info!(
        db_type = ?payload.db_type,
        host = %payload.host,
        database = %payload.database,
        "Opening connection"
    );

    let mode = if payload.read_only {
        ConnectionMode::ReadOnly
    } else {
        ConnectionMode::ReadWrite
    };

    let tls = if payload.tls_enabled {
        TlsConfig::required()
    } else {
        TlsConfig { mode: dbench_security::tls::TlsMode::Disabled, ..Default::default() }
    };

    match payload.db_type {
        DatabaseType::Postgres | DatabaseType::Cockroachdb => {
            use dbench_core::driver::Driver;
            let config = PostgresConfig {
                host: payload.host,
                port: payload.port.unwrap_or(5432),
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(10_000),
                application_name: Some("dbench".into()),
            };
            let driver = PostgresDriver;
            let conn = driver.connect(&config).await.map_err(|e| e.to_string())?;
            let id = state.registry.register(conn);
            let info = state.registry.list().into_iter().find(|i| i.id == id).unwrap();
            Ok(ConnectionResponse { connection_id: id.to_string(), info })
        }

        DatabaseType::Sqlite => {
            use dbench_core::driver::Driver;
            let config = SqliteConfig {
                path: payload.database,
                mode,
                wal_mode: true,
            };
            let driver = SqliteDriver;
            let conn = driver.connect(&config).await.map_err(|e| e.to_string())?;
            let id = state.registry.register(conn);
            let info = state.registry.list().into_iter().find(|i| i.id == id).unwrap();
            Ok(ConnectionResponse { connection_id: id.to_string(), info })
        }

        other => Err(format!("{other:?} driver not yet implemented. PostgreSQL and SQLite are supported.")),
    }
}

/// Test a connection without keeping it open.
#[tauri::command]
pub async fn test_connection(
    _state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<String, String> {
    use dbench_core::driver::Driver;

    let mode = if payload.read_only {
        ConnectionMode::ReadOnly
    } else {
        ConnectionMode::ReadWrite
    };

    let tls = if payload.tls_enabled {
        TlsConfig::required()
    } else {
        TlsConfig { mode: dbench_security::tls::TlsMode::Disabled, ..Default::default() }
    };

    match payload.db_type {
        DatabaseType::Postgres | DatabaseType::Cockroachdb => {
            let config = PostgresConfig {
                host: payload.host,
                port: payload.port.unwrap_or(5432),
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(5_000),
                application_name: Some("dbench-test".into()),
            };
            let driver = PostgresDriver;
            let info = driver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to PostgreSQL {} at {}:{}",
                info.server_version.unwrap_or("unknown".into()),
                info.host,
                info.port
            ))
        }

        DatabaseType::Sqlite => {
            let config = SqliteConfig {
                path: payload.database.clone(),
                mode,
                wal_mode: false,
            };
            let driver = SqliteDriver;
            driver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!("SQLite database '{}' opened successfully", payload.database))
        }

        other => Err(format!("{other:?} driver not yet implemented.")),
    }
}

/// Open a previously saved connection by replaying its config.
/// (Same as add_connection — saved configs are stored in the frontend.)
#[tauri::command]
pub async fn open_connection(
    state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<ConnectionResponse, String> {
    add_connection(state, payload).await
}

/// Close an active connection.
#[tauri::command]
pub async fn close_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;
    state.registry.remove(id).map_err(|e| e.to_string())
}

/// Remove a saved connection (closes it if active).
#[tauri::command]
pub async fn remove_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;
    // Ignore error if not active.
    let _ = state.registry.remove(id);
    Ok(())
}
