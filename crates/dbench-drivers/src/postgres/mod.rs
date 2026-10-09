//! PostgreSQL driver — uses `tokio-postgres` with a background connection task.
//!
//! Compatible with: PostgreSQL 12+, CockroachDB, Aurora/RDS, Supabase, Neon, YugabyteDB, Redshift.

mod schema;

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::{Query, QueryParam},
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::DatabaseSchema,
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a PostgreSQL connection.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    #[config(required)]
    pub host: String,
    pub port: u16,
    #[config(required)]
    pub database: String,
    #[config(required)]
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
    pub application_name: Option<String>,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 5432,
            database: String::new(),
            username: String::new(),
            password: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
            application_name: Some("dbench".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// TLS
// ---------------------------------------------------------------------------

/// Build a verifying TLS connector from the OS trust store plus an optional
/// custom CA bundle (e.g. Amazon RDS `global-bundle.pem` for Aurora/RDS).
fn tls_connector(tls: &TlsConfig) -> Result<native_tls::TlsConnector> {
    let tls_err = |e: String| CatalystError::connection_failed(DatabaseType::Postgres, "tls", e);
    let mut builder = native_tls::TlsConnector::builder();
    if let Some(path) = &tls.ca_cert_path {
        let pem = std::fs::read(path).map_err(|e| tls_err(format!("{}: {e}", path.display())))?;
        // A CA bundle file holds many certs; add each one.
        for cert in split_pem(&pem) {
            let cert =
                native_tls::Certificate::from_pem(cert).map_err(|e| tls_err(e.to_string()))?;
            builder.add_root_certificate(cert);
        }
    }
    builder.build().map_err(|e| tls_err(e.to_string()))
}

/// Split a PEM bundle into individual certificate blocks.
fn split_pem(pem: &[u8]) -> Vec<&[u8]> {
    const END: &[u8] = b"-----END CERTIFICATE-----";
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(i) = pem[start..].windows(END.len()).position(|w| w == END) {
        let end = start + i + END.len();
        out.push(&pem[start..end]);
        start = end;
    }
    out
}

/// Drive a tokio-postgres connection in the background.
fn spawn_connection<S, T>(connection: tokio_postgres::Connection<S, T>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    T: tokio_postgres::tls::TlsStream + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::error!("PostgreSQL connection error: {}", e);
        }
    });
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct PostgresDriver;

impl Driver for PostgresDriver {
    type Connection = PostgresConnection;
    type Config = PostgresConfig;

    fn name(&self) -> &'static str {
        "postgres"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Postgres
    }
    fn default_port(&self) -> Option<u16> {
        Some(5432)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;

        tracing::info!(
            host = %config.host, port = config.port,
            database = %config.database, username = %config.username,
            "Connecting to PostgreSQL"
        );

        let mut pg_config = tokio_postgres::Config::new();
        pg_config
            .host(&config.host)
            .port(config.port)
            .dbname(&config.database)
            .user(&config.username)
            .application_name(config.application_name.as_deref().unwrap_or("dbench"));

        if let Some(pass) = &config.password {
            pg_config.password(pass.as_str());
        }

        if let Some(ms) = config.connect_timeout_ms {
            pg_config.connect_timeout(std::time::Duration::from_millis(ms));
        }

        // tokio-postgres' Display is just "error connecting to server"; the cause
        // (timeout, refused, DNS, TLS) lives in source(), so include it.
        let conn_err = |e: tokio_postgres::Error| {
            let msg = match std::error::Error::source(&e) {
                Some(cause) => format!("{e}: {cause}"),
                None => e.to_string(),
            };
            CatalystError::connection_failed(DatabaseType::Postgres, &config.host, msg)
        };

        let client = if config.tls.mode == TlsMode::Disabled {
            let (client, connection) = pg_config
                .connect(tokio_postgres::NoTls)
                .await
                .map_err(conn_err)?;
            spawn_connection(connection);
            client
        } else {
            pg_config.ssl_mode(if config.tls.mode == TlsMode::Preferred {
                tokio_postgres::config::SslMode::Prefer
            } else {
                tokio_postgres::config::SslMode::Require
            });
            let connector = postgres_native_tls::MakeTlsConnector::new(tls_connector(&config.tls)?);
            let (client, connection) = pg_config.connect(connector).await.map_err(conn_err)?;
            spawn_connection(connection);
            client
        };

        // Apply read-only session if needed.
        if !config.mode.allows_writes() {
            client
                .execute("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY", &[])
                .await
                .map_err(query_err)?;
        }

        // Fetch server version.
        let full_version = client
            .query_one("SELECT version()", &[])
            .await
            .ok()
            .and_then(|r| r.try_get::<_, String>(0).ok())
            .unwrap_or_default();
        // "PostgreSQL 17.7 on …" vs "CockroachDB CCL v24.3.1 (…)".
        let cockroach = full_version.starts_with("CockroachDB");
        let server_version = full_version
            .split(' ')
            .find(|w| w.starts_with(|c: char| c.is_ascii_digit() || c == 'v'))
            .map(|v| v.trim_start_matches('v').to_string());

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Postgres,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        tracing::info!(conn_id = %info.id, "PostgreSQL connection established");

        Ok(PostgresConnection {
            info,
            cockroach,
            mode: config.mode,
            alive: true,
            client,
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

pub struct PostgresConnection {
    info: ConnectionInfo,
    /// CockroachDB speaks the Postgres protocol but not all of its SQL (e.g. EXPLAIN options).
    cockroach: bool,
    mode: ConnectionMode,
    alive: bool,
    client: tokio_postgres::Client,
}

impl Connection for PostgresConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }

        // Engine-level read-only guard.
        // Scans every statement, ignoring comments/strings; also blocks `SET …` so a
        // session-level READ ONLY can't be switched back off.
        if !self.mode.allows_writes() && dbench_core::guard::is_sql_write(&query.text) {
            return Err(CatalystError::ReadOnlyViolation);
        }

        let start = Instant::now();

        let query_text = if query.explain {
            explain_sql(&query.text, self.cockroach)
        } else {
            query.text.clone()
        };

        // Convert QueryParams to boxed ToSql values.
        let owned_params = to_pg_params(&query.params);
        let param_refs: Vec<&(dyn ToSql + Sync)> = owned_params
            .iter()
            .map(|p| p.as_ref() as &(dyn ToSql + Sync))
            .collect();

        let stmt = self.client.prepare(&query_text).await.map_err(query_err)?;

        let columns: Vec<Column> = stmt
            .columns()
            .iter()
            .map(|col| Column {
                name: col.name().to_string(),
                col_type: pg_type_to_col_type(col.type_()),
                nullable: true,
                native_type: col.type_().name().to_string(),
            })
            .collect();

        // DML vs SELECT dispatch.
        let upper = query_text.trim_start().to_uppercase();
        let is_dml = [
            "INSERT", "UPDATE", "DELETE", "TRUNCATE", "CREATE", "DROP", "ALTER",
        ]
        .iter()
        .any(|kw| upper.starts_with(kw));

        if is_dml && !query.explain {
            let rows_affected = self
                .client
                .execute(&stmt, &param_refs)
                .await
                .map_err(query_err)?;

            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: Some(rows_affected),
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            });
        }

        let rows = self
            .client
            .query(&stmt, &param_refs)
            .await
            .map_err(query_err)?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if query.explain {
            let plan = rows
                .iter()
                .filter_map(|r| {
                    // FORMAT JSON returns a `json` column, which String can't decode.
                    r.try_get::<_, serde_json::Value>(0)
                        .map(|v| v.to_string())
                        .or_else(|_| r.try_get::<_, String>(0))
                        .ok()
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                duration_ms,
                explain_plan: Some(plan),
            });
        }

        let result_rows: Vec<Row> = rows
            .iter()
            .map(|row| Row {
                values: columns
                    .iter()
                    .enumerate()
                    .map(|(i, col)| extract_pg_value(row, i, &col.col_type))
                    .collect(),
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            duration_ms,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        schema::inspect(&self.client, &self.info.database).await
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| CatalystError::ConnectionLost {
                reason: e.to_string(),
            })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        self.alive = false;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive
    }
    fn info(&self) -> &ConnectionInfo {
        &self.info
    }
    fn mode(&self) -> ConnectionMode {
        self.mode
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `EXPLAIN ANALYZE` actually runs the statement, so only reads get `ANALYZE`:
/// "Explain" on an `UPDATE`/`DELETE` must never modify data.
/// CockroachDB (same driver) has no `FORMAT JSON`; its text plan is shown as-is.
fn explain_sql(sql: &str, cockroach: bool) -> String {
    let analyze = !dbench_core::guard::is_sql_write(sql);
    match (cockroach, analyze) {
        (true, true) => format!("EXPLAIN ANALYZE {sql}"),
        (true, false) => format!("EXPLAIN {sql}"),
        (false, true) => format!("EXPLAIN (ANALYZE, FORMAT JSON) {sql}"),
        (false, false) => format!("EXPLAIN (FORMAT JSON) {sql}"),
    }
}

fn to_pg_params(params: &[QueryParam]) -> Vec<Box<dyn ToSql + Sync + Send>> {
    params
        .iter()
        .map(|p| -> Box<dyn ToSql + Sync + Send> {
            match p {
                QueryParam::Null => Box::new(Option::<String>::None),
                QueryParam::Bool(b) => Box::new(*b),
                QueryParam::Int(i) => Box::new(*i),
                QueryParam::Float(f) => Box::new(*f),
                QueryParam::Text(s) => Box::new(s.clone()),
                QueryParam::Bytes(b) => Box::new(b.clone()),
                QueryParam::Json(j) => Box::new(j.clone()),
                QueryParam::Uuid(u) => Box::new(*u),
                QueryParam::Timestamp(s) => Box::new(s.clone()),
            }
        })
        .collect()
}

fn pg_type_to_col_type(pg: &tokio_postgres::types::Type) -> ColumnType {
    use tokio_postgres::types::Type;
    match *pg {
        Type::BOOL => ColumnType::Boolean,
        Type::INT2 | Type::INT4 | Type::INT8 | Type::OID => ColumnType::Integer,
        Type::FLOAT4 | Type::FLOAT8 => ColumnType::Float,
        Type::NUMERIC => ColumnType::Decimal,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => ColumnType::Text,
        Type::BYTEA => ColumnType::Bytes,
        Type::DATE => ColumnType::Date,
        Type::TIME | Type::TIMETZ => ColumnType::Time,
        Type::TIMESTAMP | Type::TIMESTAMPTZ => ColumnType::Timestamp,
        Type::JSON | Type::JSONB => ColumnType::Json,
        Type::UUID => ColumnType::Uuid,
        _ => ColumnType::Unknown,
    }
}

/// `NUMERIC` decoded to its exact decimal string. tokio-postgres has no built-in
/// `FromSql` for it (`String` doesn't accept NUMERIC), so every value read as NULL.
struct PgNumeric(String);

impl<'a> postgres_types::FromSql<'a> for PgNumeric {
    fn from_sql(
        _: &postgres_types::Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        decode_numeric(raw)
            .map(PgNumeric)
            .ok_or_else(|| "malformed NUMERIC".into())
    }
    postgres_types::accepts!(NUMERIC);
}

/// Binary NUMERIC: ndigits, weight, sign, dscale (all 16-bit), then base-10000 digits.
fn decode_numeric(raw: &[u8]) -> Option<String> {
    let word = |i: usize| -> Option<i16> {
        Some(i16::from_be_bytes([*raw.get(i * 2)?, *raw.get(i * 2 + 1)?]))
    };
    let (ndigits, weight) = (word(0)? as usize, i32::from(word(1)?));
    let (sign, dscale) = (word(2)? as u16, word(3)? as u16 as usize);
    match sign {
        0xC000 => return Some("NaN".into()),
        0xD000 => return Some("Infinity".into()),
        0xF000 => return Some("-Infinity".into()),
        _ => {}
    }
    let digits: Vec<i16> = (0..ndigits).map(|i| word(4 + i)).collect::<Option<_>>()?;
    let group = |pos: i32| -> i16 {
        usize::try_from(pos)
            .ok()
            .and_then(|p| digits.get(p).copied())
            .unwrap_or(0)
    };

    let mut out = String::new();
    if sign == 0x4000 && digits.iter().any(|&d| d != 0) {
        out.push('-');
    }
    if weight < 0 {
        out.push('0');
    } else {
        out.push_str(&group(0).to_string());
        for pos in 1..=weight {
            out.push_str(&format!("{:04}", group(pos)));
        }
    }
    if dscale > 0 {
        let mut frac = String::new();
        let mut pos = weight + 1;
        while frac.len() < dscale {
            frac.push_str(&format!("{:04}", group(pos)));
            pos += 1;
        }
        frac.truncate(dscale);
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

/// tokio-postgres' Display for server errors is just "db error"; surface the server's
/// message, detail and hint instead.
fn query_err(e: tokio_postgres::Error) -> CatalystError {
    let msg = match e.as_db_error() {
        Some(db) => {
            let mut m = format!("{}: {}", db.severity(), db.message());
            if let Some(d) = db.detail() {
                m.push_str(&format!("\nDETAIL: {d}"));
            }
            if let Some(h) = db.hint() {
                m.push_str(&format!("\nHINT: {h}"));
            }
            m
        }
        None => match std::error::Error::source(&e) {
            Some(cause) => format!("{e}: {cause}"),
            None => e.to_string(),
        },
    };
    CatalystError::query_failed(msg)
}

/// Any column type, decoded from the binary wire format by [`pg_any`].
struct PgAny(Value);

impl<'a> postgres_types::FromSql<'a> for PgAny {
    fn from_sql(
        ty: &postgres_types::Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(PgAny(pg_any(ty, raw)))
    }
    fn accepts(_: &postgres_types::Type) -> bool {
        true
    }
}

/// Fallback decoder for types the typed path in [`extract_pg_value`] doesn't cover
/// (money, interval, network, bit, geometry, arrays, enums, extensions…). Never NULL
/// for a non-NULL value: unknown binary types fall back to text or bytes.
fn pg_any(ty: &postgres_types::Type, raw: &[u8]) -> Value {
    use postgres_types::{FromSql, Kind, Type};
    fn get<'a, T: FromSql<'a>>(ty: &Type, raw: &'a [u8]) -> Option<T> {
        T::from_sql(ty, raw).ok()
    }
    let be = |i: usize, n: usize| raw.get(i..i + n);
    let i32_at = |i| be(i, 4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let i64_at = |i| be(i, 8).map(|b| i64::from_be_bytes(b.try_into().unwrap_or([0; 8])));
    let f64_at = |i| be(i, 8).map(|b| f64::from_be_bytes(b.try_into().unwrap_or([0; 8])));

    if let Kind::Array(member) = ty.kind() {
        return decode_array(member, raw).map_or_else(|| Value::Bytes(raw.to_vec()), Value::Array);
    }
    if let Kind::Domain(base) = ty.kind() {
        return pg_any(base, raw);
    }
    let v = match *ty {
        Type::BOOL => get(ty, raw).map(Value::Bool),
        Type::INT2 => get::<i16>(ty, raw).map(|v| Value::Int(v.into())),
        Type::INT4 => get::<i32>(ty, raw).map(|v| Value::Int(v.into())),
        Type::INT8 => get::<i64>(ty, raw).map(Value::Int),
        Type::OID => get::<u32>(ty, raw).map(|v| Value::Int(v.into())),
        Type::FLOAT4 => get::<f32>(ty, raw).map(|v| Value::Float(v.into())),
        Type::FLOAT8 => get::<f64>(ty, raw).map(Value::Float),
        Type::NUMERIC => decode_numeric(raw).map(Value::Decimal),
        Type::UUID => get(ty, raw).map(Value::Uuid),
        Type::BYTEA => Some(Value::Bytes(raw.to_vec())),
        Type::DATE => get::<chrono::NaiveDate>(ty, raw).map(|d| Value::Date(d.to_string())),
        Type::TIME => get::<chrono::NaiveTime>(ty, raw).map(|t| Value::Time(t.to_string())),
        Type::TIMESTAMP => {
            get::<chrono::NaiveDateTime>(ty, raw).map(|t| Value::Timestamp(t.and_utc()))
        }
        Type::TIMESTAMPTZ => get(ty, raw).map(Value::Timestamp),
        Type::JSON | Type::JSONB => get(ty, raw).map(Value::Json),
        // Cents; assumes the usual 2-decimal lc_monetary.
        Type::MONEY => i64_at(0).map(|c| {
            let sign = if c < 0 { "-" } else { "" };
            Value::Decimal(format!("{sign}{}.{:02}", (c / 100).abs(), (c % 100).abs()))
        }),
        Type::INTERVAL => match (i64_at(0), i32_at(8), i32_at(12)) {
            (Some(us), Some(days), Some(months)) => {
                Some(Value::Text(format_interval(us, days, months)))
            }
            _ => None,
        },
        Type::TIMETZ => match (i64_at(0), i32_at(8)) {
            (Some(us), Some(zone)) => Some(Value::Time(format_timetz(us, zone))),
            _ => None,
        },
        Type::INET | Type::CIDR => decode_inet(raw).map(Value::Text),
        Type::MACADDR | Type::MACADDR8 => Some(Value::Text(
            raw.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(":"),
        )),
        Type::POINT => match (f64_at(0), f64_at(8)) {
            (Some(x), Some(y)) => Some(Value::Text(format!("({x},{y})"))),
            _ => None,
        },
        Type::BIT | Type::VARBIT => i32_at(0).map(|len| {
            let bits = (0..usize::try_from(len).unwrap_or(0))
                .map(|i| {
                    if raw
                        .get(4 + i / 8)
                        .is_some_and(|b| b & (0x80 >> (i % 8)) != 0)
                    {
                        '1'
                    } else {
                        '0'
                    }
                })
                .collect();
            Value::Text(bits)
        }),
        Type::TS_VECTOR => decode_tsvector(raw).map(Value::Text),
        _ => None,
    };
    // Enums, xml, citext, ltree and most extension types send plain UTF-8 text.
    v.unwrap_or_else(|| match std::str::from_utf8(raw) {
        Ok(t) if !t.chars().any(|c| c.is_control() && !c.is_whitespace()) => {
            Value::Text(t.to_owned())
        }
        _ => Value::Bytes(raw.to_vec()),
    })
}

/// Binary array: ndim, has_null, elem oid, (len, lbound) per dim, then length-prefixed
/// elements. Multi-dimensional arrays are flattened.
fn decode_array(member: &postgres_types::Type, raw: &[u8]) -> Option<Vec<Value>> {
    let word = |i: usize| {
        raw.get(i..i + 4)
            .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let ndim = usize::try_from(word(0)?).ok()?;
    let mut pos = 12 + ndim * 8;
    let mut out = Vec::new();
    while pos < raw.len() {
        let len = word(pos)?;
        pos += 4;
        if len < 0 {
            out.push(Value::Null);
            continue;
        }
        let end = pos + usize::try_from(len).ok()?;
        out.push(pg_any(member, raw.get(pos..end)?));
        pos = end;
    }
    Some(out)
}

fn format_interval(us: i64, days: i32, months: i32) -> String {
    let mut parts = Vec::new();
    let plural = |n: i64, unit: &str| format!("{n} {unit}{}", if n.abs() == 1 { "" } else { "s" });
    let (years, mons) = (i64::from(months) / 12, i64::from(months) % 12);
    if years != 0 {
        parts.push(plural(years, "year"));
    }
    if mons != 0 {
        parts.push(format!(
            "{mons} mon{}",
            if mons.abs() == 1 { "" } else { "s" }
        ));
    }
    if days != 0 {
        parts.push(plural(days.into(), "day"));
    }
    if us != 0 || parts.is_empty() {
        let sign = if us < 0 { "-" } else { "" };
        let us = us.unsigned_abs();
        let (h, m, s, frac) = (
            us / 3_600_000_000,
            us / 60_000_000 % 60,
            us / 1_000_000 % 60,
            us % 1_000_000,
        );
        let mut t = format!("{sign}{h:02}:{m:02}:{s:02}");
        if frac != 0 {
            t.push_str(format!(".{frac:06}").trim_end_matches('0'));
        }
        parts.push(t);
    }
    parts.join(" ")
}

/// `zone` is seconds *west* of UTC, as Postgres sends it.
fn format_timetz(us: i64, zone: i32) -> String {
    let t = chrono::NaiveTime::from_num_seconds_from_midnight_opt(
        u32::try_from(us / 1_000_000).unwrap_or(0),
        u32::try_from(us % 1_000_000 * 1000).unwrap_or(0),
    )
    .map(|t| t.to_string())
    .unwrap_or_default();
    let east = -zone;
    let sign = if east < 0 { '-' } else { '+' };
    let (h, m) = (east.abs() / 3600, east.abs() / 60 % 60);
    if m == 0 {
        format!("{t}{sign}{h:02}")
    } else {
        format!("{t}{sign}{h:02}:{m:02}")
    }
}

/// family (2 = v4, 3 = v6), bits, is_cidr, nbytes, address.
fn decode_inet(raw: &[u8]) -> Option<String> {
    let (&family, &bits, &is_cidr) = (raw.first()?, raw.get(1)?, raw.get(2)?);
    let addr = raw.get(4..)?;
    let (ip, max): (std::net::IpAddr, u8) = match family {
        2 => (<[u8; 4]>::try_from(addr).ok()?.into(), 32),
        _ => (<[u8; 16]>::try_from(addr).ok()?.into(), 128),
    };
    Some(if is_cidr != 0 || bits != max {
        format!("{ip}/{bits}")
    } else {
        ip.to_string()
    })
}

/// count, then per lexeme: NUL-terminated text, u16 position count, u16 positions.
fn decode_tsvector(raw: &[u8]) -> Option<String> {
    let count = i32::from_be_bytes(raw.get(0..4)?.try_into().ok()?);
    let mut pos = 4;
    let mut lexemes = Vec::new();
    for _ in 0..count {
        let end = pos + raw.get(pos..)?.iter().position(|&b| b == 0)?;
        let word = std::str::from_utf8(raw.get(pos..end)?).ok()?;
        let npos = usize::from(u16::from_be_bytes(
            raw.get(end + 1..end + 3)?.try_into().ok()?,
        ));
        lexemes.push(format!("'{word}'"));
        pos = end + 3 + npos * 2;
    }
    Some(lexemes.join(" "))
}

fn extract_pg_value(row: &tokio_postgres::Row, idx: usize, col_type: &ColumnType) -> Value {
    let v = extract_typed(row, idx, col_type);
    if !v.is_null() {
        return v;
    }
    // Typed decode failed or the value is NULL: the catch-all decoder tells them apart.
    row.try_get::<_, Option<PgAny>>(idx)
        .ok()
        .flatten()
        .map_or(Value::Null, |p| p.0)
}

fn extract_typed(row: &tokio_postgres::Row, idx: usize, col_type: &ColumnType) -> Value {
    match col_type {
        ColumnType::Boolean => row
            .try_get::<_, bool>(idx)
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        ColumnType::Integer => {
            if let Ok(v) = row.try_get::<_, i64>(idx) {
                return Value::Int(v);
            }
            if let Ok(v) = row.try_get::<_, i32>(idx) {
                return Value::Int(i64::from(v));
            }
            if let Ok(v) = row.try_get::<_, i16>(idx) {
                return Value::Int(i64::from(v));
            }
            row.try_get::<_, u32>(idx)
                .map(|v| Value::Int(i64::from(v)))
                .unwrap_or(Value::Null)
        }
        ColumnType::Float => {
            if let Ok(v) = row.try_get::<_, f64>(idx) {
                return Value::Float(v);
            }
            row.try_get::<_, f32>(idx)
                .map(|v| Value::Float(f64::from(v)))
                .unwrap_or(Value::Null)
        }
        ColumnType::Decimal => row
            .try_get::<_, PgNumeric>(idx)
            .map(|n| Value::Decimal(n.0))
            .unwrap_or(Value::Null),
        ColumnType::Bytes => row
            .try_get::<_, Vec<u8>>(idx)
            .map(Value::Bytes)
            .unwrap_or(Value::Null),
        ColumnType::Date => row
            .try_get::<_, chrono::NaiveDate>(idx)
            .map(|d| Value::Date(d.to_string()))
            .unwrap_or(Value::Null),
        ColumnType::Time => row
            .try_get::<_, chrono::NaiveTime>(idx)
            .map(|t| Value::Time(t.to_string()))
            .unwrap_or(Value::Null),
        ColumnType::Timestamp => {
            if let Ok(v) = row.try_get::<_, chrono::DateTime<chrono::Utc>>(idx) {
                return Value::Timestamp(v);
            }
            row.try_get::<_, chrono::NaiveDateTime>(idx)
                .map(|dt| Value::Timestamp(dt.and_utc()))
                .unwrap_or(Value::Null)
        }
        ColumnType::Json => row
            .try_get::<_, serde_json::Value>(idx)
            .map(Value::Json)
            .unwrap_or(Value::Null),
        ColumnType::Uuid => row
            .try_get::<_, uuid::Uuid>(idx)
            .map(Value::Uuid)
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<_, String>(idx)
            .map(Value::Text)
            .unwrap_or(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_inet, decode_numeric, explain_sql, format_interval, format_timetz, split_pem,
    };

    #[test]
    fn decodes_fallback_types() {
        assert_eq!(format_interval(2 * 3_600_000_000, 1, 0), "1 day 02:00:00");
        assert_eq!(
            format_interval(1_500_000, 0, 14),
            "1 year 2 mons 00:00:01.5"
        );
        assert_eq!(format_interval(-60_000_000, 0, 0), "-00:01:00");
        assert_eq!(format_interval(0, 0, 0), "00:00:00");
        assert_eq!(
            format_timetz((13 * 3600 + 45 * 60 + 1) * 1_000_000, -7200),
            "13:45:01+02"
        );
        assert_eq!(format_timetz(0, 19_800), "00:00:00-05:30");
        assert_eq!(
            decode_inet(&[2, 32, 0, 4, 10, 0, 0, 1]).unwrap(),
            "10.0.0.1"
        );
        assert_eq!(
            decode_inet(&[2, 8, 1, 4, 10, 0, 0, 0]).unwrap(),
            "10.0.0.0/8"
        );
    }

    #[test]
    fn explain_never_analyzes_writes() {
        assert_eq!(explain_sql("SELECT 1", true), "EXPLAIN ANALYZE SELECT 1");
        assert_eq!(explain_sql("DELETE FROM t", true), "EXPLAIN DELETE FROM t");
        assert!(explain_sql("SELECT * FROM t", false).starts_with("EXPLAIN (ANALYZE,"));
        for w in [
            "DELETE FROM t",
            "UPDATE t SET a = 1",
            "WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x",
        ] {
            assert!(
                explain_sql(w, false).starts_with("EXPLAIN (FORMAT JSON)"),
                "{w}"
            );
        }
    }

    /// Encode like Postgres' binary NUMERIC send format.
    fn numeric(weight: i16, sign: u16, dscale: u16, digits: &[i16]) -> Vec<u8> {
        let mut b = Vec::new();
        for w in [digits.len() as i16, weight, sign as i16, dscale as i16] {
            b.extend_from_slice(&w.to_be_bytes());
        }
        for d in digits {
            b.extend_from_slice(&d.to_be_bytes());
        }
        b
    }

    #[test]
    fn decodes_numeric() {
        let d = |w, s, sc, g: &[i16]| decode_numeric(&numeric(w, s, sc, g)).unwrap();
        assert_eq!(d(0, 0, 2, &[1234, 5600]), "1234.56");
        assert_eq!(d(1, 0, 0, &[12, 3456]), "123456");
        assert_eq!(d(1, 0x4000, 2, &[1, 0, 2500]), "-10000.25");
        assert_eq!(d(-1, 0, 4, &[5]), "0.0005");
        assert_eq!(d(-2, 0, 8, &[12]), "0.00000012");
        assert_eq!(d(0, 0, 2, &[]), "0.00");
        assert_eq!(d(2, 0, 0, &[1]), "100000000");
        assert_eq!(d(0, 0xC000, 0, &[]), "NaN");
    }

    #[test]
    fn split_pem_returns_each_certificate() {
        let pem = b"-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----\n";
        let certs = split_pem(pem);
        assert_eq!(certs.len(), 2);
        assert!(certs[1].ends_with(b"-----END CERTIFICATE-----"));
        assert!(split_pem(b"garbage").is_empty());
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::PostgresConfig;

    #[test]
    fn debug_never_prints_password() {
        let cfg = PostgresConfig {
            password: Some("hunter2".into()),
            ..PostgresConfig::default()
        };
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("hunter2"), "{dbg}");
        assert!(dbg.contains("password=[REDACTED]"));
    }
}
