//! Tauri commands for connection management.

use dbench_core::{
    query::Query,
    result::Value,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
};
use dbench_drivers::{
    cassandra::{CassandraConfig, CassandraDriver},
    clickhouse::{ClickhouseConfig, ClickhouseDriver},
    mongodb::{MongoConfig, MongoDriver},
    mysql::{MysqlConfig, MysqlDriver},
    postgres::{PostgresConfig, PostgresDriver},
    redis::{RedisConfig, RedisDriver},
    sqlite::{SqliteConfig, SqliteDriver},
};
use dbench_security::{
    keychain::KeychainStore,
    ssh::{SshAuthMethod, SshTunnel, SshTunnelConfig},
    tls::{TlsConfig, TlsMode},
};
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
    // Optional SSH tunnel fields
    pub ssh_enabled: Option<bool>,
    pub ssh_host: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_username: Option<String>,
    pub ssh_auth_method: Option<String>, // "agent" | "key"
    pub ssh_key_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub connection_id: String,
    pub info: ConnectionInfo,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_tls(enabled: bool) -> TlsConfig {
    if enabled {
        TlsConfig::required()
    } else {
        TlsConfig { mode: TlsMode::Disabled, ..Default::default() }
    }
}

/// Establish an SSH tunnel if the payload requests one.
/// Returns `(effective_host, effective_port, Option<SshTunnel>)`.
async fn maybe_ssh(
    payload: &ConnectionPayload,
) -> Result<(String, u16, Option<SshTunnel>), String> {
    if payload.ssh_enabled != Some(true) {
        let port = payload.port.unwrap_or_else(|| {
            payload.db_type.default_port().unwrap_or(5432)
        });
        return Ok((payload.host.clone(), port, None));
    }

    let ssh_host = payload.ssh_host.clone()
        .filter(|s| !s.is_empty())
        .ok_or("SSH host is required")?;
    let ssh_username = payload.ssh_username.clone()
        .filter(|s| !s.is_empty())
        .ok_or("SSH username is required")?;
    let remote_host = payload.host.clone();
    let remote_port = payload.port.unwrap_or_else(|| {
        payload.db_type.default_port().unwrap_or(5432)
    });

    let auth = match payload.ssh_auth_method.as_deref().unwrap_or("agent") {
        "key" => {
            let key_path = payload.ssh_key_path.clone()
                .filter(|s| !s.is_empty())
                .ok_or("SSH key path is required for key auth")?;
            SshAuthMethod::PrivateKey { key_path: key_path.into(), passphrase_keychain_id: None }
        }
        _ => SshAuthMethod::Agent,
    };

    let tunnel_config = SshTunnelConfig {
        ssh_host,
        ssh_port: payload.ssh_port.unwrap_or(22),
        ssh_username,
        auth,
        remote_host,
        remote_port,
    };

    let tunnel = SshTunnel::connect(tunnel_config).await.map_err(|e| e.to_string())?;
    let local_host = tunnel.local_host().to_string();
    let local_port = tunnel.local_port();
    Ok((local_host, local_port, Some(tunnel)))
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
        ssh = payload.ssh_enabled.unwrap_or(false),
        "Opening connection"
    );

    let mode = if payload.read_only { ConnectionMode::ReadOnly } else { ConnectionMode::ReadWrite };
    let tls = build_tls(payload.tls_enabled);

    let (eff_host, eff_port, ssh_tunnel) = maybe_ssh(&payload).await?;

    macro_rules! register {
        ($conn:expr) => {{
            let id = state.registry.register($conn);
            if let Some(tunnel) = ssh_tunnel {
                state.ssh_tunnels.insert(id, tunnel);
            }
            let info = state.registry.list().into_iter().find(|i| i.id == id).unwrap();
            Ok(ConnectionResponse { connection_id: id.to_string(), info })
        }};
    }

    match payload.db_type {
        DatabaseType::Postgres | DatabaseType::Cockroachdb => {
            let config = PostgresConfig {
                host: eff_host,
                port: eff_port,
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
                host: eff_host,
                port: eff_port,
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
            // URI mode is incompatible with SSH tunneling; when a tunnel is active
            // the driver must connect to 127.0.0.1:<local_port>.
            let ssh_active = payload.ssh_enabled.unwrap_or(false);
            let is_uri = !ssh_active
                && (payload.host.starts_with("mongodb://")
                    || payload.host.starts_with("mongodb+srv://"));
            let config = MongoConfig {
                uri: if is_uri { Some(payload.host.clone()) } else { None },
                host: if is_uri { "localhost".into() } else { eff_host },
                port: eff_port,
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
                host: eff_host,
                port: eff_port,
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
                host: eff_host,
                port: eff_port,
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

        DatabaseType::Cassandra => {
            let config = CassandraConfig {
                host: eff_host,
                port: eff_port,
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(10_000),
            };
            let conn = CassandraDriver.connect(&config).await.map_err(|e| e.to_string())?;
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

        DatabaseType::Cassandra => {
            let config = CassandraConfig {
                host: payload.host.clone(),
                port: payload.port.unwrap_or(9042),
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(5_000),
            };
            let info = CassandraDriver.test_connection(&config).await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected to Cassandra {} at {}:{}",
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
    // Drop the SSH tunnel first (kills the child process via Drop).
    state.ssh_tunnels.remove(&id);
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

/// List databases available on an active connection.
///
/// Returns a sorted list of database names. For databases that don't support
/// multiple databases (SQLite, Redis), returns the current database/index only.
#[tauri::command]
pub async fn list_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    let conn_id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;

    let db_type = state
        .registry
        .list()
        .into_iter()
        .find(|i| i.id == conn_id)
        .map(|i| i.db_type)
        .ok_or_else(|| format!("Connection {connection_id} not found"))?;

    match db_type {
        DatabaseType::Sqlite => {
            // SQLite is a single-file database — return the file path as the sole entry.
            let db = state
                .registry
                .list()
                .into_iter()
                .find(|i| i.id == conn_id)
                .map(|i| i.database)
                .unwrap_or_default();
            return Ok(vec![db]);
        }
        DatabaseType::Redis => {
            // Redis logical databases are numbered 0-15 by default.
            return Ok((0u8..=15).map(|n| n.to_string()).collect());
        }
        DatabaseType::Mongodb => {
            // MongoDB: list databases via the listDatabases admin command.
            let query = Query::new(r#"{"command":{"listDatabases":1},"db":"admin"}"#);
            let result = state
                .executor
                .execute(conn_id, query)
                .await
                .map_err(|e| e.to_string())?;

            let names: Vec<String> = result
                .rows
                .into_iter()
                .filter_map(|row| row.values.into_iter().next())
                .filter_map(|v| match v {
                    Value::Text(s) => Some(s),
                    _ => None,
                })
                .collect();
            return Ok(names);
        }
        _ => {}
    }

    // SQL databases: run the appropriate query.
    let sql = match db_type {
        DatabaseType::Postgres | DatabaseType::Cockroachdb => {
            "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname"
        }
        DatabaseType::Mysql => "SHOW DATABASES",
        DatabaseType::Clickhouse => "SHOW DATABASES",
        DatabaseType::Cassandra => {
            "SELECT keyspace_name FROM system_schema.keyspaces \
             WHERE keyspace_name NOT IN \
             ('system','system_auth','system_distributed','system_traces',\
             'system_views','system_virtual_schema')"
        }
        _ => return Err(format!("{db_type:?} does not support listing databases")),
    };

    let query = Query::new(sql);

    let result = state
        .executor
        .execute(conn_id, query)
        .await
        .map_err(|e| e.to_string())?;

    let names: Vec<String> = result
        .rows
        .into_iter()
        .filter_map(|row| row.values.into_iter().next())
        .filter_map(|v| match v {
            Value::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    Ok(names)
}
