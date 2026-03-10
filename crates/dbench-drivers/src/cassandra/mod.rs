//! Cassandra / ScyllaDB driver — uses the `scylla` native CQL driver.
//!
//! Compatible with: Apache Cassandra 3.x/4.x/5.x, ScyllaDB, Amazon Keyspaces.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::Query,
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::{ColumnSchema, DatabaseSchema, ForeignKeySchema, IndexSchema, SchemaObject, TableSchema},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use serde::{Deserialize, Serialize};
#[allow(deprecated)]
use scylla::{Session, SessionBuilder};
use scylla::frame::response::result::CqlValue;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a Cassandra / ScyllaDB connection.
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct CassandraConfig {
    #[config(required)]
    pub host: String,
    pub port: u16,
    /// Default keyspace (optional — can be omitted to inspect all keyspaces).
    pub database: String,
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
}

impl Default for CassandraConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 9042,
            database: String::new(),
            username: String::new(),
            password: None,
            tls: TlsConfig { mode: TlsMode::Disabled, ..Default::default() },
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct CassandraDriver;

impl Driver for CassandraDriver {
    type Connection = CassandraConnection;
    type Config = CassandraConfig;

    fn name(&self) -> &'static str { "cassandra" }
    fn database_type(&self) -> DatabaseType { DatabaseType::Cassandra }
    fn default_port(&self) -> Option<u16> { Some(9042) }

    #[allow(deprecated)]
    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        tracing::info!(
            host = %config.host,
            port = config.port,
            keyspace = %config.database,
            "Connecting to Cassandra"
        );

        let addr = format!("{}:{}", config.host, config.port);

        let mut builder = SessionBuilder::new().known_node(&addr);

        if !config.username.is_empty() {
            let password = config.password.as_deref().unwrap_or("");
            builder = builder.user(&config.username, password);
        }

        if let Some(ms) = config.connect_timeout_ms {
            builder = builder.connection_timeout(Duration::from_millis(ms));
        }

        if !config.database.is_empty() {
            builder = builder.use_keyspace(&config.database, false);
        }

        let session = builder
            .build()
            .await
            .map_err(|e| CatalystError::connection_failed(
                DatabaseType::Cassandra, &config.host, e.to_string(),
            ))?;

        let session = Arc::new(session);

        // Fetch server version from system.local.
        let server_version = fetch_server_version(&session).await;

        tracing::info!(version = ?server_version, "Cassandra connection established");

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Cassandra,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        Ok(CassandraConnection {
            info,
            mode: config.mode,
            alive: true,
            session,
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct CassandraConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    session: Arc<Session>,
}

impl Connection for CassandraConnection {
    #[allow(deprecated)]
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost { reason: "connection is closed".into() });
        }

        if !self.mode.allows_writes() {
            let upper = query.text.trim_start().to_uppercase();
            let writes = ["INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE"];
            if writes.iter().any(|kw| upper.starts_with(kw)) {
                return Err(CatalystError::ReadOnlyViolation);
            }
        }

        let start = Instant::now();

        let result = self
            .session
            .query(query.text.as_str(), &[])
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        let col_specs = result.col_specs.clone();

        let columns: Vec<Column> = col_specs
            .iter()
            .map(|spec| Column {
                name: spec.name.clone(),
                col_type: cql_type_to_col_type(&spec.typ),
                nullable: true,
                native_type: format!("{:?}", spec.typ),
            })
            .collect();

        let result_rows: Vec<Row> = result
            .rows
            .unwrap_or_default()
            .into_iter()
            .map(|row| Row {
                values: row.columns.into_iter().map(cql_to_value).collect(),
            })
            .collect();

        let rows_affected = if columns.is_empty() && result_rows.is_empty() {
            Some(0)
        } else {
            None
        };

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected,
            duration_ms,
            explain_plan: None,
        })
    }

    #[allow(deprecated)]
    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        inspect_schema(&self.session, &self.info).await
    }

    #[allow(deprecated)]
    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.session
            .query("SELECT key FROM system.local WHERE key = 'local'", &[])
            .await
            .map_err(|e| CatalystError::ConnectionLost { reason: e.to_string() })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        self.alive = false;
        Ok(())
    }

    fn is_alive(&self) -> bool { self.alive }
    fn info(&self) -> &ConnectionInfo { &self.info }
    fn mode(&self) -> ConnectionMode { self.mode }
}

// ---------------------------------------------------------------------------
// Schema introspection
// ---------------------------------------------------------------------------

#[allow(deprecated)]
async fn inspect_schema(session: &Session, info: &ConnectionInfo) -> Result<DatabaseSchema> {
    // Fetch all user-defined keyspaces.
    let ks_result = session
        .query(
            "SELECT keyspace_name FROM system_schema.keyspaces \
             WHERE keyspace_name NOT IN ('system', 'system_auth', 'system_distributed', \
             'system_traces', 'system_views', 'system_virtual_schema') \
             ALLOW FILTERING",
            &[],
        )
        .await
        .map_err(|e| CatalystError::query_failed(e.to_string()))?;

    let keyspaces: Vec<String> = ks_result
        .rows
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            row.columns
                .into_iter()
                .next()
                .flatten()
                .and_then(|v| if let CqlValue::Text(s) = v { Some(s) } else { None })
        })
        .collect();

    // For each keyspace, fetch its tables and their columns.
    let mut objects: Vec<SchemaObject> = Vec::new();

    for keyspace in &keyspaces {
        // Tables in this keyspace.
        let table_result = session
            .query(
                "SELECT table_name FROM system_schema.tables WHERE keyspace_name = ?",
                (keyspace.as_str(),),
            )
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?;

        let table_names: Vec<String> = table_result
            .rows
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| {
                row.columns
                    .into_iter()
                    .next()
                    .flatten()
                    .and_then(|v| if let CqlValue::Text(s) = v { Some(s) } else { None })
            })
            .collect();

        for table_name in table_names {
            let col_result = session
                .query(
                    "SELECT column_name, type, kind, position \
                     FROM system_schema.columns \
                     WHERE keyspace_name = ? AND table_name = ? \
                     ORDER BY position",
                    (keyspace.as_str(), table_name.as_str()),
                )
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            let mut columns: Vec<ColumnSchema> = Vec::new();
            for row in col_result.rows.unwrap_or_default() {
                let mut cols = row.columns.into_iter();
                let col_name = cols
                    .next()
                    .flatten()
                    .and_then(|v| if let CqlValue::Text(s) = v { Some(s) } else { None })
                    .unwrap_or_default();
                let col_type = cols
                    .next()
                    .flatten()
                    .and_then(|v| if let CqlValue::Text(s) = v { Some(s) } else { None })
                    .unwrap_or_default();
                let kind = cols
                    .next()
                    .flatten()
                    .and_then(|v| if let CqlValue::Text(s) = v { Some(s) } else { None })
                    .unwrap_or_default();
                let position = cols
                    .next()
                    .flatten()
                    .and_then(|v| match v {
                        CqlValue::Int(i) => Some(i as u32),
                        _ => None,
                    })
                    .unwrap_or(0);

                let is_primary_key = kind == "partition_key" || kind == "clustering";

                columns.push(ColumnSchema {
                    name: col_name,
                    ordinal: position,
                    native_type: col_type,
                    nullable: !is_primary_key,
                    default_value: None,
                    is_primary_key,
                    is_unique: is_primary_key,
                    comment: Some(kind),
                });
            }

            objects.push(SchemaObject::Table(TableSchema {
                schema: Some(keyspace.clone()),
                name: table_name,
                columns,
                indexes: Vec::<IndexSchema>::new(),
                foreign_keys: Vec::<ForeignKeySchema>::new(),
                row_count: None,
                comment: None,
            }));
        }
    }

    let db_name = if info.database.is_empty() {
        "cassandra".to_string()
    } else {
        info.database.clone()
    };

    Ok(DatabaseSchema {
        name: db_name,
        db_type: DatabaseType::Cassandra,
        server_version: info.server_version.clone().unwrap_or_else(|| "unknown".into()),
        objects,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(deprecated)]
async fn fetch_server_version(session: &Session) -> Option<String> {
    let result = session
        .query("SELECT release_version FROM system.local WHERE key = 'local'", &[])
        .await
        .ok()?;
    result
        .rows?
        .into_iter()
        .next()?
        .columns
        .into_iter()
        .next()
        .flatten()
        .and_then(|v| if let CqlValue::Text(s) = v { Some(s) } else { None })
}

fn cql_type_to_col_type(cql: &scylla::frame::response::result::ColumnType) -> ColumnType {
    use scylla::frame::response::result::ColumnType as CT;
    match cql {
        CT::Boolean => ColumnType::Boolean,
        CT::TinyInt | CT::SmallInt | CT::Int | CT::BigInt | CT::Counter => ColumnType::Integer,
        CT::Float | CT::Double => ColumnType::Float,
        CT::Decimal | CT::Varint => ColumnType::Decimal,
        CT::Ascii | CT::Text => ColumnType::Text,
        CT::Blob => ColumnType::Bytes,
        CT::Date => ColumnType::Date,
        CT::Time => ColumnType::Time,
        CT::Timestamp => ColumnType::Timestamp,
        CT::Uuid | CT::Timeuuid => ColumnType::Uuid,
        CT::Inet => ColumnType::Text,
        CT::List(_) | CT::Set(_) => ColumnType::Unknown,
        CT::Map(_, _) => ColumnType::Json,
        CT::Tuple(_) => ColumnType::Json,
        CT::UserDefinedType { .. } => ColumnType::Json,
        _ => ColumnType::Unknown,
    }
}

fn cql_to_value(v: Option<CqlValue>) -> Value {
    match v {
        None => Value::Null,
        Some(CqlValue::Ascii(s)) | Some(CqlValue::Text(s)) => Value::Text(s),
        Some(CqlValue::Boolean(b)) => Value::Bool(b),
        Some(CqlValue::TinyInt(i)) => Value::Int(i64::from(i)),
        Some(CqlValue::SmallInt(i)) => Value::Int(i64::from(i)),
        Some(CqlValue::Int(i)) => Value::Int(i64::from(i)),
        Some(CqlValue::BigInt(i)) => Value::Int(i),
        Some(CqlValue::Counter(c)) => Value::Int(c.0),
        Some(CqlValue::Float(f)) => Value::Float(f64::from(f)),
        Some(CqlValue::Double(d)) => Value::Float(d),
        Some(CqlValue::Decimal(d)) => Value::Decimal(format!("{d:?}")),
        Some(CqlValue::Varint(v)) => Value::Decimal(format!("{v:?}")),
        Some(CqlValue::Date(d)) => Value::Date(format!("{d:?}")),
        Some(CqlValue::Time(t)) => Value::Time(format!("{t:?}")),
        Some(CqlValue::Timestamp(ts)) => {
            let dt = chrono::DateTime::from_timestamp_millis(ts.0)
                .unwrap_or_default();
            Value::Timestamp(dt)
        }
        Some(CqlValue::Uuid(u)) => Value::Uuid(u),
        Some(CqlValue::Timeuuid(u)) => Value::Uuid(Uuid::from(u)),
        Some(CqlValue::Blob(b)) => Value::Bytes(b),
        Some(CqlValue::Inet(addr)) => Value::Text(addr.to_string()),
        Some(CqlValue::List(list)) => {
            let arr: Vec<Value> = list.into_iter().map(|c| cql_to_value(Some(c))).collect();
            Value::Array(arr)
        }
        Some(CqlValue::Set(set)) => {
            let arr: Vec<Value> = set.into_iter().map(|c| cql_to_value(Some(c))).collect();
            Value::Array(arr)
        }
        Some(CqlValue::Map(map)) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| {
                    let key = match &k {
                        CqlValue::Text(s) | CqlValue::Ascii(s) => s.clone(),
                        other => format!("{other:?}"),
                    };
                    let val = serde_json::Value::String(format!("{v:?}"));
                    (key, val)
                })
                .collect();
            Value::Json(serde_json::Value::Object(obj))
        }
        Some(CqlValue::Tuple(fields)) => {
            let arr: Vec<Value> = fields.into_iter().map(cql_to_value).collect();
            Value::Array(arr)
        }
        Some(CqlValue::UserDefinedType { fields, .. }) => {
            let obj: serde_json::Map<String, serde_json::Value> = fields
                .into_iter()
                .map(|(name, val)| (name, serde_json::Value::String(format!("{val:?}"))))
                .collect();
            Value::Json(serde_json::Value::Object(obj))
        }
        Some(CqlValue::Empty) => Value::Null,
        Some(other) => Value::Text(format!("{other:?}")),
    }
}
