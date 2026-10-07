//! Tauri commands for connection management.

use dbench_core::{
    query::Query,
    result::Value,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
};
use dbench_drivers::{
    CassandraConfig, CassandraDriver, ClickhouseConfig, ClickhouseDriver, DynamoConfig,
    DynamoDriver, ElasticsearchConfig, ElasticsearchDriver, MongoConfig, MongoDriver, MssqlConfig,
    MssqlDriver, MysqlConfig, MysqlDriver, OracleConfig, OracleDriver, PostgresConfig,
    PostgresDriver, RedisConfig, RedisDriver, SqliteConfig, SqliteDriver, SurrealConfig,
    SurrealDriver,
};
use dbench_engine::registry::{BoxConnection, ConnectionAdapter};
use dbench_security::{
    audit::{AuditEvent, CloseReason},
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
/// Deliberately not `Debug`: it carries the plaintext password.
#[derive(Deserialize)]
pub struct ConnectionPayload {
    pub db_type: DatabaseType,
    pub host: String,
    pub port: Option<u16>,
    pub database: String,
    pub username: String,
    pub password: Option<String>,
    pub tls_enabled: bool,
    /// Optional CA bundle (PEM) used to verify the server certificate, e.g. the Amazon RDS bundle.
    pub tls_ca_path: Option<String>,
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

fn build_tls(payload: &ConnectionPayload) -> TlsConfig {
    if payload.tls_enabled {
        TlsConfig {
            ca_cert_path: payload
                .tls_ca_path
                .as_deref()
                .filter(|p| !p.is_empty())
                .map(Into::into),
            ..TlsConfig::required()
        }
    } else {
        TlsConfig {
            mode: TlsMode::Disabled,
            ..Default::default()
        }
    }
}

/// Establish an SSH tunnel if the payload requests one.
/// Returns `(effective_host, effective_port, Option<SshTunnel>)`.
async fn maybe_ssh(
    payload: &ConnectionPayload,
) -> Result<(String, u16, Option<SshTunnel>), String> {
    if payload.ssh_enabled != Some(true) {
        let port = payload
            .port
            .unwrap_or_else(|| payload.db_type.default_port().unwrap_or(5432));
        return Ok((payload.host.clone(), port, None));
    }

    let ssh_host = payload
        .ssh_host
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or("SSH host is required")?;
    let ssh_username = payload
        .ssh_username
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or("SSH username is required")?;
    let remote_host = payload.host.clone();
    let remote_port = payload
        .port
        .unwrap_or_else(|| payload.db_type.default_port().unwrap_or(5432));

    let auth = match payload.ssh_auth_method.as_deref().unwrap_or("agent") {
        "key" => {
            let key_path = payload
                .ssh_key_path
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or("SSH key path is required for key auth")?;
            SshAuthMethod::PrivateKey {
                key_path: key_path.into(),
                passphrase_keychain_id: None,
            }
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

    let tunnel = SshTunnel::connect(tunnel_config)
        .await
        .map_err(|e| e.to_string())?;
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

/// Build the driver config for `payload` and connect through `host:port`
/// (the payload's own address, or the local end of an SSH tunnel).
async fn open(
    payload: ConnectionPayload,
    host: String,
    port: u16,
    timeout_ms: u64,
) -> Result<BoxConnection, String> {
    use dbench_core::driver::Driver;

    let mode = if payload.read_only {
        ConnectionMode::ReadOnly
    } else {
        ConnectionMode::ReadWrite
    };
    let tls = build_tls(&payload);
    let ssh_active = payload.ssh_enabled.unwrap_or(false);
    let opt = |s: String| if s.is_empty() { None } else { Some(s) };

    macro_rules! connect {
        ($driver:expr, $config:expr) => {{
            let conn = $driver.connect(&$config).await.map_err(|e| e.to_string())?;
            Ok(Box::new(ConnectionAdapter(conn)) as BoxConnection)
        }};
    }

    match payload.db_type {
        DatabaseType::Postgres | DatabaseType::Cockroachdb => connect!(
            PostgresDriver,
            PostgresConfig {
                host,
                port,
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(timeout_ms),
                application_name: Some("catalyst-dbench".into()),
            }
        ),
        DatabaseType::Mysql => connect!(
            MysqlDriver,
            MysqlConfig {
                host,
                port,
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(timeout_ms),
            }
        ),
        DatabaseType::Sqlite => connect!(
            SqliteDriver,
            SqliteConfig {
                path: payload.database,
                mode,
                wal_mode: true
            }
        ),
        DatabaseType::Mongodb => {
            // URI mode is incompatible with SSH tunneling; when a tunnel is active
            // the driver must connect to 127.0.0.1:<local_port>.
            let is_uri = !ssh_active
                && (payload.host.starts_with("mongodb://")
                    || payload.host.starts_with("mongodb+srv://"));
            connect!(
                MongoDriver,
                MongoConfig {
                    uri: if is_uri {
                        Some(payload.host.clone())
                    } else {
                        None
                    },
                    host: if is_uri { "localhost".into() } else { host },
                    port,
                    database: payload.database,
                    username: opt(payload.username),
                    password: payload.password,
                    auth_source: None,
                    replica_set: None,
                    tls,
                    mode,
                    connect_timeout_ms: Some(timeout_ms),
                }
            )
        }
        DatabaseType::Redis => connect!(
            RedisDriver,
            RedisConfig {
                host,
                port,
                db_index: 0,
                password: payload.password,
                username: opt(payload.username),
                tls: payload.tls_enabled,
                mode,
            }
        ),
        DatabaseType::Clickhouse => connect!(
            ClickhouseDriver,
            ClickhouseConfig {
                host,
                port,
                database: opt(payload.database).unwrap_or_else(|| "default".into()),
                username: opt(payload.username).unwrap_or_else(|| "default".into()),
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(timeout_ms),
            }
        ),
        DatabaseType::Cassandra => connect!(
            CassandraDriver,
            CassandraConfig {
                host,
                port,
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(timeout_ms),
            }
        ),
        DatabaseType::Mssql => connect!(
            MssqlDriver,
            MssqlConfig {
                host,
                port,
                database: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(timeout_ms),
            }
        ),
        DatabaseType::Oracle => connect!(
            OracleDriver,
            OracleConfig {
                host,
                port,
                service: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
            }
        ),
        DatabaseType::Elasticsearch => connect!(
            ElasticsearchDriver,
            ElasticsearchConfig {
                host,
                port,
                index_pattern: payload.database,
                username: payload.username,
                password: payload.password,
                tls,
                mode,
                connect_timeout_ms: Some(timeout_ms),
            }
        ),
        DatabaseType::Surrealdb => {
            let (namespace, database) = SurrealConfig::split_target(&payload.database);
            connect!(
                SurrealDriver,
                SurrealConfig {
                    host,
                    port,
                    namespace,
                    database,
                    username: payload.username,
                    password: payload.password,
                    tls,
                    mode,
                    connect_timeout_ms: Some(timeout_ms),
                }
            )
        }
        DatabaseType::Dynamodb => {
            if ssh_active {
                return Err(
                    "SSH tunnels are not supported for DynamoDB (it is an HTTPS API).".into(),
                );
            }
            // Host is either a region (`us-east-1`) or an endpoint URL (DynamoDB Local).
            let (region, endpoint) =
                if payload.host.starts_with("http://") || payload.host.starts_with("https://") {
                    (
                        opt(payload.database).unwrap_or_else(|| "us-east-1".into()),
                        Some(payload.host),
                    )
                } else {
                    (
                        opt(payload.host).unwrap_or_else(|| "us-east-1".into()),
                        None,
                    )
                };
            connect!(
                DynamoDriver,
                DynamoConfig {
                    region,
                    endpoint,
                    access_key_id: payload.username,
                    secret_access_key: payload.password,
                    mode,
                }
            )
        }
        other => Err(format!("{other:?} is not yet supported.")),
    }
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
        ssh = payload.ssh_enabled.unwrap_or(false),
        "Opening connection"
    );

    let (host, port, ssh_tunnel) = maybe_ssh(&payload).await?;
    let conn = open(payload, host, port, 10_000).await?;
    let mut info = conn.info().clone();
    info.ssh_tunnel = ssh_tunnel.is_some();
    let id = state.registry.register_boxed(conn, info.clone());
    if let Some(tunnel) = ssh_tunnel {
        state.ssh_tunnels.insert(id, tunnel);
    }
    let _ = state
        .audit
        .log(AuditEvent::ConnectionOpened {
            conn_id: id.to_string(),
            db_type: format!("{:?}", info.db_type).to_lowercase(),
            host: info.host.clone(),
            port: info.port,
            database: info.database.clone(),
            tls: info.tls_active,
            ssh_tunnel: info.ssh_tunnel,
        })
        .await;
    Ok(ConnectionResponse {
        connection_id: id.to_string(),
        info,
    })
}

/// Test a connection (through the SSH tunnel, if configured) without keeping it open.
#[tauri::command]
pub async fn test_connection(
    _state: State<'_, AppState>,
    payload: ConnectionPayload,
) -> Result<String, String> {
    let db_type = payload.db_type;
    // The tunnel (if any) lives until the end of this function, then is torn down.
    let (host, port, _tunnel) = maybe_ssh(&payload).await?;
    let mut conn = open(payload, host, port, 5_000).await?;
    conn.ping().await.map_err(|e| e.to_string())?;
    let info = conn.info();
    Ok(format!(
        "Connected to {} {}",
        db_type.display_name(),
        info.server_version.clone().unwrap_or_default()
    )
    .trim_end()
    .to_string())
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
    state.registry.remove(id).map_err(|e| e.to_string())?;
    let _ = state
        .audit
        .log(AuditEvent::ConnectionClosed {
            conn_id: id.to_string(),
            reason: CloseReason::UserInitiated,
        })
        .await;
    Ok(())
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
    use dbench_security::KeychainError;
    use dbench_security::SecurityError;

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
        // One logical database per connection: the switcher shows just the current one.
        DatabaseType::Sqlite
        | DatabaseType::Oracle
        | DatabaseType::Elasticsearch
        | DatabaseType::Surrealdb
        | DatabaseType::Dynamodb => {
            // Return the connection's own database/file/service/region as the sole entry.
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
        DatabaseType::Mssql => {
            "SELECT name FROM sys.databases WHERE HAS_DBACCESS(name) = 1 ORDER BY name"
        }
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
