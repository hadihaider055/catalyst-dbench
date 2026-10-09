//! Microsoft SQL Server / Azure SQL driver via `tiberius` (TDS 7.3+, rustls).

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    guard::{is_sql_write, strip_comments_and_strings},
    query::Query,
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::DatabaseSchema,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use tiberius::{AuthMethod, Client, ColumnData, EncryptionLevel, FromSql, QueryItem};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use uuid::Uuid;

use crate::sqlmeta::{build_objects, ColRow, FkRow};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a SQL Server connection.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct MssqlConfig {
    pub host: String,
    /// TDS port (default: 1433).
    pub port: u16,
    /// Initial database (empty = login default).
    pub database: String,
    #[config(required)]
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
}

impl Default for MssqlConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 1433,
            database: String::new(),
            username: "sa".into(),
            password: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

type MssqlClient = Client<Compat<TcpStream>>;

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// SQL Server driver.
#[derive(Debug, Default)]
pub struct MssqlDriver;

impl Driver for MssqlDriver {
    type Connection = MssqlConnection;
    type Config = MssqlConfig;

    fn name(&self) -> &'static str {
        "mssql"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Mssql
    }
    fn default_port(&self) -> Option<u16> {
        Some(1433)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        let fail =
            |e: String| CatalystError::connection_failed(DatabaseType::Mssql, &config.host, e);

        let mut cfg = tiberius::Config::new();
        cfg.host(&config.host);
        cfg.port(config.port);
        if !config.database.is_empty() {
            cfg.database(&config.database);
        }
        cfg.application_name("catalyst-dbench");
        cfg.authentication(AuthMethod::sql_server(
            &config.username,
            config.password.as_deref().unwrap_or_default(),
        ));
        if config.tls.mode == TlsMode::Disabled {
            cfg.encryption(EncryptionLevel::NotSupported);
        } else {
            // Certificates are validated against system roots, or the given CA.
            cfg.encryption(EncryptionLevel::Required);
            if let Some(ca) = &config.tls.ca_cert_path {
                cfg.trust_cert_ca(ca.to_string_lossy());
            }
        }

        let timeout = Duration::from_millis(config.connect_timeout_ms.unwrap_or(10_000));
        let mut client = tokio::time::timeout(timeout, open(cfg))
            .await
            .map_err(|_| fail(format!("timed out after {}ms", timeout.as_millis())))?
            .map_err(fail)?;

        let server_version = scalar(
            &mut client,
            "SELECT CAST(SERVERPROPERTY('ProductVersion') AS nvarchar(128))",
        )
        .await;
        let database = scalar(&mut client, "SELECT DB_NAME()")
            .await
            .unwrap_or_else(|| config.database.clone());

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Mssql,
            host: config.host.clone(),
            port: config.port,
            database,
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };
        tracing::info!(conn_id = %info.id, "SQL Server connection established");
        Ok(MssqlConnection {
            info,
            mode: config.mode,
            client: Some(client),
        })
    }
}

/// Connect, following one Azure SQL gateway redirect.
async fn open(mut cfg: tiberius::Config) -> std::result::Result<MssqlClient, String> {
    let tcp = TcpStream::connect(cfg.get_addr())
        .await
        .map_err(|e| e.to_string())?;
    tcp.set_nodelay(true).map_err(|e| e.to_string())?;
    match Client::connect(cfg.clone(), tcp.compat_write()).await {
        Ok(c) => Ok(c),
        Err(tiberius::error::Error::Routing { host, port }) => {
            cfg.host(&host);
            cfg.port(port);
            let tcp = TcpStream::connect(cfg.get_addr())
                .await
                .map_err(|e| e.to_string())?;
            tcp.set_nodelay(true).map_err(|e| e.to_string())?;
            Client::connect(cfg, tcp.compat_write())
                .await
                .map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

async fn scalar(client: &mut MssqlClient, sql: &str) -> Option<String> {
    let row = client
        .simple_query(sql)
        .await
        .ok()?
        .into_row()
        .await
        .ok()??;
    row.get::<&str, _>(0).map(str::to_owned)
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// A live SQL Server connection.
pub struct MssqlConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    client: Option<MssqlClient>,
}

impl MssqlConnection {
    fn client(&mut self) -> Result<&mut MssqlClient> {
        self.client
            .as_mut()
            .ok_or_else(|| CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            })
    }

    /// Run a batch and return every result set as (columns, rows).
    async fn run(&mut self, sql: &str) -> Result<Vec<(Vec<Column>, Vec<Row>)>> {
        let mut stream = self
            .client()?
            .simple_query(sql)
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?;
        let mut sets: Vec<(Vec<Column>, Vec<Row>)> = Vec::new();
        while let Some(item) = stream
            .try_next()
            .await
            .map_err(|e| CatalystError::query_failed(e.to_string()))?
        {
            match item {
                QueryItem::Metadata(meta) => sets.push((
                    meta.columns()
                        .iter()
                        .map(|c| Column {
                            name: c.name().to_string(),
                            col_type: map_type(c.column_type()),
                            nullable: true,
                            native_type: format!("{:?}", c.column_type()).to_lowercase(),
                        })
                        .collect(),
                    vec![],
                )),
                QueryItem::Row(row) => {
                    if let Some((_, rows)) = sets.last_mut() {
                        rows.push(Row {
                            values: row.into_iter().map(to_value).collect(),
                        });
                    }
                }
            }
        }
        Ok(sets)
    }
}

/// T-SQL runs a batch's first statement as a procedure call even without `EXEC`
/// (`sp_rename 'a', 'b'`, `xp_cmdshell '…'`), so anything not opening with a
/// known read keyword is treated as a write on read-only connections.
fn is_implicit_exec(sql: &str) -> bool {
    let stripped = strip_comments_and_strings(sql);
    let first = stripped
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .find(|w| !w.is_empty());
    !first.is_some_and(|w| ["SELECT", "WITH"].iter().any(|k| k.eq_ignore_ascii_case(w)))
}

impl Connection for MssqlConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        // SQL Server has no read-only session switch; enforce client-side.
        if !self.mode.allows_writes()
            && (is_sql_write(&query.text) || is_implicit_exec(&query.text))
        {
            return Err(CatalystError::ReadOnlyViolation);
        }
        let start = Instant::now();
        let sets = self.run(&query.text).await?;
        // Show the last result set, like SSMS's final grid.
        let (columns, rows) = sets.into_iter().last().unwrap_or_default();
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            duration_ms: start.elapsed().as_millis() as u64,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let text = |v: Option<&Value>| match v {
            Some(Value::Text(s)) => s.clone(),
            Some(Value::Int(n)) => n.to_string(),
            _ => String::new(),
        };
        let cols = self
            .run(COLUMNS_SQL)
            .await
            .map_err(|e| CatalystError::SchemaError(e.to_string()))?;
        let fks = self
            .run(FKS_SQL)
            .await
            .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

        let col_rows = cols
            .into_iter()
            .next()
            .map(|s| s.1)
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let v = &r.values;
                ColRow {
                    schema: text(v.first()),
                    table: text(v.get(1)),
                    is_view: text(v.get(2)) == "VIEW",
                    column: text(v.get(3)),
                    native_type: text(v.get(4)),
                    nullable: text(v.get(5)) == "YES",
                    default_value: Some(text(v.get(6))).filter(|s| !s.is_empty()),
                    is_pk: matches!(v.get(7), Some(Value::Int(1))),
                }
            })
            .collect();
        let fk_rows = fks
            .into_iter()
            .next()
            .map(|s| s.1)
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let v = &r.values;
                FkRow {
                    name: text(v.first()),
                    schema: text(v.get(1)),
                    table: text(v.get(2)),
                    column: text(v.get(3)),
                    ref_table: text(v.get(4)),
                    ref_column: text(v.get(5)),
                }
            })
            .collect();

        Ok(DatabaseSchema {
            name: self.info.database.clone(),
            db_type: DatabaseType::Mssql,
            server_version: self.info.server_version.clone().unwrap_or_default(),
            objects: build_objects(col_rows, fk_rows),
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.run("SELECT 1")
            .await
            .map_err(|e| CatalystError::ConnectionLost {
                reason: e.to_string(),
            })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        if let Some(c) = self.client.take() {
            c.close().await.map_err(|e| CatalystError::ConnectionLost {
                reason: e.to_string(),
            })?;
        }
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.client.is_some()
    }
    fn info(&self) -> &ConnectionInfo {
        &self.info
    }
    fn mode(&self) -> ConnectionMode {
        self.mode
    }
}

// ---------------------------------------------------------------------------
// Catalog queries
// ---------------------------------------------------------------------------

const COLUMNS_SQL: &str = "\
SELECT c.TABLE_SCHEMA, c.TABLE_NAME, CASE WHEN t.TABLE_TYPE = 'VIEW' THEN 'VIEW' ELSE 'TABLE' END,
       c.COLUMN_NAME,
       c.DATA_TYPE + CASE WHEN c.CHARACTER_MAXIMUM_LENGTH = -1 THEN '(max)'
                          WHEN c.CHARACTER_MAXIMUM_LENGTH IS NOT NULL THEN '(' + CAST(c.CHARACTER_MAXIMUM_LENGTH AS varchar(10)) + ')'
                          ELSE '' END,
       c.IS_NULLABLE, c.COLUMN_DEFAULT,
       CAST(CASE WHEN k.COLUMN_NAME IS NULL THEN 0 ELSE 1 END AS bigint)
FROM INFORMATION_SCHEMA.COLUMNS c
JOIN INFORMATION_SCHEMA.TABLES t ON t.TABLE_SCHEMA = c.TABLE_SCHEMA AND t.TABLE_NAME = c.TABLE_NAME
LEFT JOIN (SELECT ku.TABLE_SCHEMA, ku.TABLE_NAME, ku.COLUMN_NAME
           FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc
           JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE ku
             ON tc.CONSTRAINT_NAME = ku.CONSTRAINT_NAME AND tc.TABLE_SCHEMA = ku.TABLE_SCHEMA
           WHERE tc.CONSTRAINT_TYPE = 'PRIMARY KEY') k
  ON k.TABLE_SCHEMA = c.TABLE_SCHEMA AND k.TABLE_NAME = c.TABLE_NAME AND k.COLUMN_NAME = c.COLUMN_NAME
WHERE c.TABLE_SCHEMA NOT IN ('sys', 'INFORMATION_SCHEMA')
ORDER BY c.TABLE_SCHEMA, c.TABLE_NAME, c.ORDINAL_POSITION";

const FKS_SQL: &str = "\
SELECT fk.name, SCHEMA_NAME(tp.schema_id), tp.name, cp.name, tr.name, cr.name
FROM sys.foreign_keys fk
JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id
JOIN sys.tables tp ON tp.object_id = fkc.parent_object_id
JOIN sys.columns cp ON cp.object_id = fkc.parent_object_id AND cp.column_id = fkc.parent_column_id
JOIN sys.tables tr ON tr.object_id = fkc.referenced_object_id
JOIN sys.columns cr ON cr.object_id = fkc.referenced_object_id AND cr.column_id = fkc.referenced_column_id
ORDER BY fk.name, fkc.constraint_column_id";

// ---------------------------------------------------------------------------
// Type mapping
// ---------------------------------------------------------------------------

fn map_type(t: tiberius::ColumnType) -> ColumnType {
    use tiberius::ColumnType as T;
    match t {
        T::Int1 | T::Int2 | T::Int4 | T::Int8 | T::Intn => ColumnType::Integer,
        T::Float4 | T::Float8 | T::Floatn => ColumnType::Float,
        T::Decimaln | T::Numericn | T::Money | T::Money4 => ColumnType::Decimal,
        T::Bit | T::Bitn => ColumnType::Boolean,
        T::Guid => ColumnType::Uuid,
        T::Daten => ColumnType::Date,
        T::Timen => ColumnType::Time,
        T::Datetime | T::Datetime4 | T::Datetimen | T::Datetime2 | T::DatetimeOffsetn => {
            ColumnType::Timestamp
        }
        T::BigBinary | T::BigVarBin | T::Image => ColumnType::Bytes,
        _ => ColumnType::Text,
    }
}

fn to_value(d: ColumnData<'static>) -> Value {
    let ts = |d: &ColumnData<'static>| {
        chrono::NaiveDateTime::from_sql(d)
            .ok()
            .flatten()
            .map_or(Value::Null, |dt| Value::Timestamp(dt.and_utc()))
    };
    match &d {
        ColumnData::U8(v) => v.map_or(Value::Null, |n| Value::Int(n.into())),
        ColumnData::I16(v) => v.map_or(Value::Null, |n| Value::Int(n.into())),
        ColumnData::I32(v) => v.map_or(Value::Null, |n| Value::Int(n.into())),
        ColumnData::I64(v) => v.map_or(Value::Null, Value::Int),
        ColumnData::F32(v) => v.map_or(Value::Null, |n| Value::Float(n.into())),
        ColumnData::F64(v) => v.map_or(Value::Null, Value::Float),
        ColumnData::Bit(v) => v.map_or(Value::Null, Value::Bool),
        ColumnData::String(v) => v
            .as_ref()
            .map_or(Value::Null, |s| Value::Text(s.to_string())),
        ColumnData::Guid(v) => v.map_or(Value::Null, Value::Uuid),
        ColumnData::Binary(v) => v.as_ref().map_or(Value::Null, |b| Value::Bytes(b.to_vec())),
        ColumnData::Numeric(v) => v.map_or(Value::Null, |n| Value::Decimal(n.to_string())),
        ColumnData::Xml(v) => v
            .as_ref()
            .map_or(Value::Null, |x| Value::Text(x.to_string())),
        ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => ts(&d),
        // tiberius' `DateTime<Utc>` decode subtracts the offset from a value that is
        // already UTC on the wire; its `FixedOffset` decode is correct.
        ColumnData::DateTimeOffset(_) => chrono::DateTime::<chrono::FixedOffset>::from_sql(&d)
            .ok()
            .flatten()
            .map_or(Value::Null, |t| Value::Timestamp(t.to_utc())),
        ColumnData::Date(_) => chrono::NaiveDate::from_sql(&d)
            .ok()
            .flatten()
            .map_or(Value::Null, |x| Value::Date(x.to_string())),
        ColumnData::Time(_) => chrono::NaiveTime::from_sql(&d)
            .ok()
            .flatten()
            .map_or(Value::Null, |x| Value::Time(x.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::is_implicit_exec;

    #[test]
    fn bare_procedure_call_is_not_a_read() {
        assert!(!is_implicit_exec("SELECT * FROM t"));
        assert!(!is_implicit_exec(
            "-- c\n;WITH x AS (SELECT 1 a) SELECT a FROM x"
        ));
        assert!(!is_implicit_exec("(SELECT 1) UNION (SELECT 2)"));
        assert!(is_implicit_exec("sp_rename 'a', 'b'"));
        assert!(is_implicit_exec("xp_cmdshell 'whoami'"));
        assert!(is_implicit_exec("[dbo].[purge_all]"));
        assert!(is_implicit_exec("/* SELECT */ master..sp_who"));
    }
}
