//! Tauri commands for connection management.

use dbench_core::types::{ConnectionInfo, ConnectionMode, DatabaseType};
use dbench_drivers::{
    clickhouse::{ClickhouseConfig, ClickhouseDriver},
    mongodb::{MongoConfig, MongoDriver},
    mysql::{MysqlConfig, MysqlDriver},
    postgres::{PostgresConfig, PostgresDriver},
    redis::{RedisConfig, RedisDriver},
    sqlite::{SqliteConfig, SqliteDriver},
};
use dbench_security::{keychain::KeychainStore, tls::TlsConfig};
use secrecy::Secret;
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
    use dbench_core::driver::Driver;

    tracing::info!(
        db_type = ?payload.db_type,
        host = %payload.host,
        database = %payload.database,
        "Opening connection"
    );

    let mode = if payload.read_only { ConnectionMode::ReadOnly } else { ConnectionMode::ReadWrite };
    let tls = if payload.tls_enabled {
        TlsConfig::required()
    } else {
        TlsConfig { mode: dbench_security::tls::TlsMode::Disabled, ..Default::default() }
    };

    macro_rules! register {
        ($conn:expr) => {{
            let id = state.registry.register($conn);
            let info = state.registry.list().into_iter().find(|i| i.id == id).unwrap();
            Ok(ConnectionResponse { connection_id: id.to_string(), info })
        }};
    }

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
                connect_timeout_ms: Some(10_000),
                application_name: Some("catalyst-dbench".into()),
            };
            let conn = PostgresDriver.connect(&config).await.map_err(|e| e.to_string())?;
            register!(conn)
        }

        DatabaseType::Mysql => {
            let config = MysqlConfig {
                host: payload.host,
                port: payload.port.unwrap_or(3306),
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(10_000),
            };
            let conn = MysqlDriver.connect(&config).await.map_err(|e| e.to_string())?;
            register!(conn)
        }

        DatabaseType::Sqlite => {
            let config = SqliteConfig {
                path: payload.database,
                mode,
                wal_mode: true,
            };
            let conn = SqliteDriver.connect(&config).await.map_err(|e| e.to_string())?;
            register!(conn)
        }

        DatabaseType::Mongodb => {
            let is_uri = payload.host.starts_with("mongodb://") || payload.host.starts_with("mongodb+srv://");
            let config = MongoConfig {
                uri: if is_uri { Some(payload.host.clone()) } else { None },
                host: if is_uri { "localhost".into() } else { payload.host },
                port: payload.port.unwrap_or(27017),
                database: payload.database,
                username: if payload.username.is_empty() { None } else { Some(payload.username) },
                password: payload.password,
                auth_source: None,
                replica_set: None,
                tls,
                mode,
                connect_timeout_ms: Some(10_000),
            };
            let conn = MongoDriver.connect(&config).await.map_err(|e| e.to_string())?;
            register!(conn)
        }

        DatabaseType::Redis => {
            let config = RedisConfig {
                host: payload.host,
                port: payload.port.unwrap_or(6379),
                db_index: 0,
                password: payload.password,
                username: if payload.username.is_empty() { None } else { Some(payload.username) },
                tls: payload.tls_enabled,
                mode,
            };
            let conn = RedisDriver.connect(&config).await.map_err(|e| e.to_string())?;
            register!(conn)
        }

        DatabaseType::Clickhouse => {
            let config = ClickhouseConfig {
                host: payload.host,
                port: payload.port.unwrap_or(8123),
                database: if payload.database.is_empty() { "default".into() } else { payload.database },
                username: if payload.username.is_empty() { "default".into() } else { payload.username },
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(10_000),
            };
            let conn = ClickhouseDriver.connect(&config).await.map_err(|e| e.to_string())?;
            register!(conn)
        }

        other => Err(format!("{other:?} is not yet supported.")),
    }
}

/// Test a connection without keeping it open.
#[tauri::command]
pub async fn test_connection(
    _state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<String, String> {
    use dbench_core::driver::Driver;

    let mode = if payload.read_only { ConnectionMode::ReadOnly } else { ConnectionMode::ReadWrite };
    let tls = if payload.tls_enabled {
        TlsConfig::required()
    } else {
        TlsConfig { mode: dbench_security::tls::TlsMode::Disabled, ..Default::default() }
    };

    match payload.db_type {
        DatabaseType::Postgres | DatabaseType::Cockroachdb => {
            let config = PostgresConfig {
                host: payload.host.clone(),
                port: payload.port.unwrap_or(5432),
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(5_000),
                application_name: Some("catalyst-dbench-test".into()),
            };
            let info = PostgresDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to PostgreSQL {} at {}:{}",
                info.server_version.unwrap_or_else(|| "unknown".into()),
                info.host, info.port
            ))
        }

        DatabaseType::Mysql => {
            let config = MysqlConfig {
                host: payload.host.clone(),
                port: payload.port.unwrap_or(3306),
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(5_000),
            };
            let info = MysqlDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to MySQL {} at {}:{}",
                info.server_version.unwrap_or_else(|| "unknown".into()),
                info.host, info.port
            ))
        }

        DatabaseType::Sqlite => {
            let config = SqliteConfig {
                path: payload.database.clone(),
                mode,
                wal_mode: false,
            };
            SqliteDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!("SQLite database '{}' opened successfully", payload.database))
        }

        DatabaseType::Mongodb => {
            let is_uri = payload.host.starts_with("mongodb://") || payload.host.starts_with("mongodb+srv://");
            let config = MongoConfig {
                uri: if is_uri { Some(payload.host.clone()) } else { None },
                host: if is_uri { "localhost".into() } else { payload.host.clone() },
                port: payload.port.unwrap_or(27017),
                database: payload.database,
                username: if payload.username.is_empty() { None } else { Some(payload.username) },
                password: payload.password,
                auth_source: None,
                replica_set: None,
                tls,
                mode,
                connect_timeout_ms: Some(5_000),
            };
            let info = MongoDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to MongoDB {} at {}:{}",
                info.server_version.unwrap_or_else(|| "unknown".into()),
                info.host, info.port
            ))
        }

        DatabaseType::Redis => {
            let config = RedisConfig {
                host: payload.host.clone(),
                port: payload.port.unwrap_or(6379),
                db_index: 0,
                password: payload.password,
                username: if payload.username.is_empty() { None } else { Some(payload.username) },
                tls: payload.tls_enabled,
                mode,
            };
            let info = RedisDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to Redis {} at {}:{}",
                info.server_version.unwrap_or_else(|| "unknown".into()),
                info.host, info.port
            ))
        }

        DatabaseType::Clickhouse => {
            let config = ClickhouseConfig {
                host: payload.host.clone(),
                port: payload.port.unwrap_or(8123),
                database: if payload.database.is_empty() { "default".into() } else { payload.database },
                username: if payload.username.is_empty() { "default".into() } else { payload.username },
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(5_000),
            };
            let info = ClickhouseDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to ClickHouse {} at {}:{}",
                info.server_version.unwrap_or_else(|| "unknown".into()),
                info.host, info.port
            ))
        }

        other => Err(format!("{other:?} is not yet supported.")),
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
    let _ = state.registry.remove(id);
    // Best-effort keychain cleanup — ignore errors (entry may not exist).
    let _ = KeychainStore::for_password(&connection_id).delete();
    Ok(())
}

/// Store a connection password in the OS keychain.
///
/// Called by the frontend after a successful connect so the user doesn't have
/// to re-enter the password on every reconnect.
#[tauri::command]
pub async fn store_credential(connection_id: String, password: String) -> Result<(), String> {
    KeychainStore::for_password(&connection_id)
        .store(&Secret::new(password))
        .map_err(|e| e.to_string())
}

/// Retrieve a connection password from the OS keychain.
///
/// Returns `None` when no credential is stored (first connect, or keychain cleared).
#[tauri::command]
pub async fn get_credential(connection_id: String) -> Result<Option<String>, String> {
    use dbench_security::SecurityError;
    use dbench_security::KeychainError;

    match KeychainStore::for_password(&connection_id).retrieve() {
        Ok(secret) => {
            use secrecy::ExposeSecret;
            Ok(Some(secret.expose_secret().clone()))
        }
        Err(SecurityError::Keychain(KeychainError::NotFound { .. })) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
