//! SurrealDB driver over the HTTP `/sql` endpoint (SurrealDB 1.x and 2.x).
//!
//! The "database" field takes `namespace/database` (or a single name used for both).

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    guard::is_sql_write,
    query::Query,
    result::{Column, ColumnType, QueryResult, Row},
    schema::{ColumnSchema, DatabaseSchema, SchemaObject, TableSchema},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

use crate::http::{build_client, json_to_value, rows_from_objects};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for a SurrealDB server.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct SurrealConfig {
    pub host: String,
    /// HTTP port (default: 8000).
    pub port: u16,
    #[config(required)]
    pub namespace: String,
    #[config(required)]
    pub database: String,
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
}

impl Default for SurrealConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8000,
            namespace: "test".into(),
            database: "test".into(),
            username: "root".into(),
            password: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

impl SurrealConfig {
    /// Split a `namespace/database` string; a single name is used for both.
    #[must_use]
    pub fn split_target(target: &str) -> (String, String) {
        match target.split_once('/') {
            Some((ns, db)) => (ns.trim().into(), db.trim().into()),
            None => (target.trim().into(), target.trim().into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// SurrealDB driver.
#[derive(Debug, Default)]
pub struct SurrealDriver;

impl Driver for SurrealDriver {
    type Connection = SurrealConnection;
    type Config = SurrealConfig;

    fn name(&self) -> &'static str {
        "surrealdb"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Surrealdb
    }
    fn default_port(&self) -> Option<u16> {
        Some(8000)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        let fail =
            |e: String| CatalystError::connection_failed(DatabaseType::Surrealdb, &config.host, e);

        let timeout = Duration::from_millis(config.connect_timeout_ms.unwrap_or(10_000));
        let client = build_client(&config.tls, timeout).map_err(fail)?;
        let scheme = if config.tls.mode == TlsMode::Disabled {
            "http"
        } else {
            "https"
        };
        let base = Url::parse(&format!("{scheme}://{}:{}/", config.host, config.port))
            .map_err(|e| CatalystError::Config(format!("invalid host: {e}")))?;

        let mut conn = SurrealConnection {
            info: ConnectionInfo {
                id: Uuid::new_v4(),
                db_type: DatabaseType::Surrealdb,
                host: config.host.clone(),
                port: config.port,
                database: format!("{}/{}", config.namespace, config.database),
                username: config.username.clone(),
                tls_active: config.tls.mode != TlsMode::Disabled,
                ssh_tunnel: false,
                server_version: None,
                connected_at: chrono::Utc::now(),
            },
            mode: config.mode,
            alive: true,
            client,
            base,
            namespace: config.namespace.clone(),
            database: config.database.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
        };

        // Authenticates as well as checking reachability.
        conn.sql("RETURN true;", &[]).await.map_err(fail)?;
        conn.info.server_version = conn.version().await;

        tracing::info!(conn_id = %conn.info.id, "SurrealDB connection established");
        Ok(conn)
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// A live SurrealDB connection (stateless HTTP client).
pub struct SurrealConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    client: Client,
    base: Url,
    namespace: String,
    database: String,
    username: String,
    password: Option<String>,
}

impl SurrealConnection {
    /// Run SurrealQL; `vars` are bound as `$name` variables. Returns each statement's result.
    async fn sql(
        &self,
        text: &str,
        vars: &[(&str, &str)],
    ) -> std::result::Result<Vec<Json>, String> {
        let mut url = self.base.join("sql").map_err(|e| e.to_string())?;
        url.query_pairs_mut().extend_pairs(vars);
        let mut req = self
            .client
            .post(url)
            .header("Accept", "application/json")
            // 2.x headers, plus the 1.x names for older servers.
            .header("surreal-ns", &self.namespace)
            .header("surreal-db", &self.database)
            .header("NS", &self.namespace)
            .header("DB", &self.database)
            .body(text.to_owned());
        if !self.username.is_empty() {
            req = req.basic_auth(&self.username, self.password.as_ref());
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        let parsed: Json =
            serde_json::from_str(&body).map_err(|_| format!("HTTP {status}: {}", body.trim()))?;
        if !status.is_success() {
            let msg = parsed["information"]
                .as_str()
                .or(parsed["details"].as_str())
                .unwrap_or(body.trim());
            return Err(format!("HTTP {status}: {msg}"));
        }
        let mut out = Vec::new();
        for stmt in parsed.as_array().cloned().unwrap_or_default() {
            if stmt["status"].as_str() == Some("ERR") {
                return Err(stmt["result"]
                    .as_str()
                    .unwrap_or("statement failed")
                    .to_string());
            }
            out.push(stmt["result"].clone());
        }
        Ok(out)
    }

    async fn version(&self) -> Option<String> {
        let url = self.base.join("version").ok()?;
        let text = self.client.get(url).send().await.ok()?.text().await.ok()?;
        Some(text.trim().replace("surrealdb-", "SurrealDB "))
    }
}

impl Connection for SurrealConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }
        // SurrealDB has no read-only session; block writing statements client-side.
        if !self.mode.allows_writes() && is_sql_write(&query.text) {
            return Err(CatalystError::ReadOnlyViolation);
        }
        let start = Instant::now();
        let results = self
            .sql(&query.text, &[])
            .await
            .map_err(CatalystError::query_failed)?;
        let mut result = to_result(results.into_iter().last().unwrap_or(Json::Null));
        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let info = self
            .sql("INFO FOR DB;", &[])
            .await
            .map_err(CatalystError::SchemaError)?;
        let info = info.into_iter().next().unwrap_or(Json::Null);
        // 2.x: "tables"; 1.x: "tb".
        let tables = info
            .get("tables")
            .or_else(|| info.get("tb"))
            .and_then(Json::as_object)
            .cloned()
            .unwrap_or_default();

        let mut objects = Vec::new();
        for name in tables.keys() {
            // Table name is bound as a variable, never spliced into the query.
            let sample = self
                .sql("SELECT * FROM type::table($tb) LIMIT 20;", &[("tb", name)])
                .await
                .map_err(CatalystError::SchemaError)?;
            let docs: Vec<_> = sample
                .into_iter()
                .next()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|d| d.as_object().cloned())
                .collect();
            let (cols, _) = rows_from_objects(&docs);
            objects.push(SchemaObject::Table(TableSchema {
                schema: None,
                name: name.clone(),
                columns: cols
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| ColumnSchema {
                        is_primary_key: c.name == "id",
                        is_unique: c.name == "id",
                        name: c.name,
                        ordinal: i as u32,
                        native_type: c.native_type,
                        nullable: true,
                        default_value: None,
                        comment: None,
                    })
                    .collect(),
                indexes: vec![],
                foreign_keys: vec![],
                row_count: None,
                comment: None,
            }));
        }

        Ok(DatabaseSchema {
            name: self.info.database.clone(),
            db_type: DatabaseType::Surrealdb,
            server_version: self.info.server_version.clone().unwrap_or_default(),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.sql("RETURN true;", &[])
            .await
            .map_err(|reason| CatalystError::ConnectionLost { reason })?;
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

/// Records → table rows; a single object → one row; scalars → one `result` cell.
fn to_result(v: Json) -> QueryResult {
    let (columns, rows) = match v {
        Json::Array(items) if items.iter().all(Json::is_object) && !items.is_empty() => {
            let objs: Vec<_> = items
                .into_iter()
                .filter_map(|d| d.as_object().cloned())
                .collect();
            rows_from_objects(&objs)
        }
        Json::Object(o) => rows_from_objects(&[o]),
        Json::Array(items) if items.is_empty() => (vec![], vec![]),
        other => (
            vec![Column {
                name: "result".into(),
                col_type: ColumnType::Json,
                nullable: true,
                native_type: "any".into(),
            }],
            vec![Row {
                values: vec![json_to_value(other)],
            }],
        ),
    };
    QueryResult {
        columns,
        rows,
        rows_affected: None,
        duration_ms: 0,
        explain_plan: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_splitting() {
        assert_eq!(
            SurrealConfig::split_target("prod/app"),
            ("prod".into(), "app".into())
        );
        assert_eq!(
            SurrealConfig::split_target("test"),
            ("test".into(), "test".into())
        );
    }

    #[test]
    fn read_only_blocks_surrealql_writes() {
        for q in [
            "CREATE person SET name = 'x'",
            "RELATE a->b->c",
            "DEFINE TABLE t",
            "REMOVE TABLE t",
            "SELECT 1; DELETE person",
        ] {
            assert!(is_sql_write(q), "{q}");
        }
        assert!(!is_sql_write(
            "SELECT * FROM person WHERE name = 'DELETE me'"
        ));
    }
}
