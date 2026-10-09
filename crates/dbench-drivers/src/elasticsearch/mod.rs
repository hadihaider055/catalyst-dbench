//! Elasticsearch / OpenSearch driver over the REST API.
//!
//! Two query styles are accepted in the editor:
//! - **SQL** — `SELECT * FROM logs LIMIT 10` → `_sql` (Elasticsearch) or `_plugins/_sql` (OpenSearch).
//! - **Console** — Kibana Dev Tools syntax: first line `METHOD /path`, optional JSON body below.

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
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use uuid::Uuid;

use crate::http::{build_client, json_to_value, json_type, rows_from_objects};

/// Endpoints that only read, allowed for `POST` on read-only connections.
const READ_ENDPOINTS: &[&str] = &[
    "_search",
    "_msearch",
    "_count",
    "_mget",
    "_sql",
    "_field_caps",
    "_validate",
    "_explain",
    "_terms_enum",
    "_analyze",
];

/// Write endpoints: a `POST` touching any of these is never a read, even when
/// another segment looks like one (`/logs/_doc/_search` indexes a doc with id `_search`).
const WRITE_ENDPOINTS: &[&str] = &[
    "_doc",
    "_create",
    "_update",
    "_bulk",
    "_delete_by_query",
    "_update_by_query",
    "_reindex",
];

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for an Elasticsearch or OpenSearch cluster.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct ElasticsearchConfig {
    pub host: String,
    /// HTTP port (default: 9200).
    pub port: u16,
    /// Optional index pattern to limit the schema browser (e.g. `logs-*`). Empty = all.
    pub index_pattern: String,
    /// Basic-auth username. Leave empty to send `password` as an API key.
    pub username: String,
    /// Basic-auth password, or an API key when `username` is empty.
    #[config(secret)]
    pub password: Option<String>,
    pub tls: TlsConfig,
    pub mode: ConnectionMode,
    pub connect_timeout_ms: Option<u64>,
}

impl Default for ElasticsearchConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 9200,
            index_pattern: String::new(),
            username: String::new(),
            password: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// Elasticsearch / OpenSearch driver.
#[derive(Debug, Default)]
pub struct ElasticsearchDriver;

impl Driver for ElasticsearchDriver {
    type Connection = ElasticsearchConnection;
    type Config = ElasticsearchConfig;

    fn name(&self) -> &'static str {
        "elasticsearch"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Elasticsearch
    }
    fn default_port(&self) -> Option<u16> {
        Some(9200)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        let fail = |e: String| {
            CatalystError::connection_failed(DatabaseType::Elasticsearch, &config.host, e)
        };

        let timeout = Duration::from_millis(config.connect_timeout_ms.unwrap_or(10_000));
        let client = build_client(&config.tls, timeout).map_err(fail)?;
        let scheme = if config.tls.mode == TlsMode::Disabled {
            "http"
        } else {
            "https"
        };
        let base = Url::parse(&format!("{scheme}://{}:{}/", config.host, config.port))
            .map_err(|e| CatalystError::Config(format!("invalid host: {e}")))?;

        let mut conn = ElasticsearchConnection {
            info: ConnectionInfo {
                id: Uuid::new_v4(),
                db_type: DatabaseType::Elasticsearch,
                host: config.host.clone(),
                port: config.port,
                database: config.index_pattern.clone(),
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
            auth: Auth::from(config),
            opensearch: false,
        };

        let root = conn.request(Method::GET, "/", None).await.map_err(fail)?;
        let version = &root["version"];
        conn.opensearch = version["distribution"].as_str() == Some("opensearch");
        conn.info.server_version = version["number"].as_str().map(|v| {
            if conn.opensearch {
                format!("OpenSearch {v}")
            } else {
                format!("Elasticsearch {v}")
            }
        });

        tracing::info!(conn_id = %conn.info.id, opensearch = conn.opensearch, "Search connection established");
        Ok(conn)
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Auth {
    None,
    Basic(String, Option<String>),
    ApiKey(String),
}

impl From<&ElasticsearchConfig> for Auth {
    fn from(c: &ElasticsearchConfig) -> Self {
        match (&c.username, &c.password) {
            (u, p) if !u.is_empty() => Self::Basic(u.clone(), p.clone()),
            (_, Some(key)) if !key.is_empty() => Self::ApiKey(key.clone()),
            _ => Self::None,
        }
    }
}

/// A live Elasticsearch / OpenSearch connection (stateless HTTP client).
pub struct ElasticsearchConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    client: Client,
    base: Url,
    auth: Auth,
    opensearch: bool,
}

impl ElasticsearchConnection {
    /// Send a request to `path` (relative to the cluster root) and parse the JSON reply.
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Json>,
    ) -> std::result::Result<Json, String> {
        // `join` on a path that starts with "/" stays on the configured host.
        let url = self
            .base
            .join(path.trim_start_matches('/'))
            .map_err(|e| e.to_string())?;
        if url.host_str() != self.base.host_str()
            || url.port_or_known_default() != self.base.port_or_known_default()
        {
            return Err("request path must stay on the configured cluster".into());
        }
        let mut req = self.client.request(method, url);
        req = match &self.auth {
            Auth::None => req,
            Auth::Basic(u, p) => req.basic_auth(u, p.as_ref()),
            Auth::ApiKey(k) => req.header("Authorization", format!("ApiKey {k}")),
        };
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let reason = serde_json::from_str::<Json>(&text)
                .ok()
                .and_then(|j| j["error"]["reason"].as_str().map(str::to_owned))
                .unwrap_or(text);
            return Err(format!("HTTP {status}: {reason}"));
        }
        if text.trim().is_empty() {
            return Ok(Json::Null);
        }
        // `_cat` APIs without ?format=json return plain text.
        Ok(serde_json::from_str(&text).unwrap_or(Json::String(text)))
    }

    async fn run_sql(&self, sql: &str) -> Result<QueryResult> {
        if !self.mode.allows_writes() && is_sql_write(sql) {
            return Err(CatalystError::ReadOnlyViolation);
        }
        let sql = sql.trim().trim_end_matches(';');
        let start = Instant::now();
        let (path, body) = if self.opensearch {
            ("/_plugins/_sql?format=jdbc", json!({ "query": sql }))
        } else {
            (
                "/_sql?format=json",
                json!({ "query": sql, "fetch_size": 1000 }),
            )
        };
        let resp = self
            .request(Method::POST, path, Some(body))
            .await
            .map_err(CatalystError::query_failed)?;

        // ES: {columns, rows}; OpenSearch JDBC: {schema, datarows}.
        let cols = resp
            .get("columns")
            .or_else(|| resp.get("schema"))
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let data = resp
            .get("rows")
            .or_else(|| resp.get("datarows"))
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let columns = cols
            .iter()
            .map(|c| {
                let native = c["type"].as_str().unwrap_or("keyword").to_string();
                Column {
                    name: c["name"].as_str().unwrap_or("").into(),
                    col_type: es_type(&native),
                    nullable: true,
                    native_type: native,
                }
            })
            .collect();
        let rows = data
            .into_iter()
            .map(|r| Row {
                values: r
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(json_to_value)
                    .collect(),
            })
            .collect();
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            duration_ms: start.elapsed().as_millis() as u64,
            explain_plan: None,
        })
    }

    async fn run_console(&self, method: Method, path: &str, body: &str) -> Result<QueryResult> {
        if !self.mode.allows_writes() && !is_read_request(&method, path) {
            return Err(CatalystError::ReadOnlyViolation);
        }
        let body = if body.trim().is_empty() {
            None
        } else {
            Some(
                serde_json::from_str(body)
                    .map_err(|e| CatalystError::query_failed(format!("Invalid JSON body: {e}")))?,
            )
        };
        let start = Instant::now();
        let resp = self
            .request(method, path, body)
            .await
            .map_err(CatalystError::query_failed)?;
        let mut result = response_to_result(resp);
        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }
}

impl Connection for ElasticsearchConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }
        match parse_console(&query.text) {
            Some((method, path, body)) => self.run_console(method, &path, body).await,
            None => self.run_sql(&query.text).await,
        }
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let pattern = if self.info.database.is_empty() {
            "*"
        } else {
            self.info.database.as_str()
        };
        // Restrict to index-name characters so the pattern can't alter the request path.
        if !pattern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.*,+".contains(c))
        {
            return Err(CatalystError::Config(format!(
                "invalid index pattern '{pattern}'"
            )));
        }
        let cat_path = format!("/_cat/indices/{pattern}?format=json&h=index,docs.count");
        let mapping_path = format!("/{pattern}/_mapping");

        let cat = self
            .request(Method::GET, &cat_path, None)
            .await
            .map_err(CatalystError::SchemaError)?;
        let mappings = self
            .request(Method::GET, &mapping_path, None)
            .await
            .map_err(CatalystError::SchemaError)?;

        let mut objects = Vec::new();
        for idx in cat.as_array().cloned().unwrap_or_default() {
            let name = idx["index"].as_str().unwrap_or_default().to_string();
            if name.is_empty() || name.starts_with('.') {
                continue; // system / hidden indices
            }
            let mut columns = Vec::new();
            flatten_mapping(&mappings[&name]["mappings"]["properties"], "", &mut columns);
            objects.push(SchemaObject::Table(TableSchema {
                schema: None,
                row_count: idx["docs.count"].as_str().and_then(|n| n.parse().ok()),
                name,
                columns,
                indexes: vec![],
                foreign_keys: vec![],
                comment: None,
            }));
        }
        objects.sort_by(|a, b| a.name().cmp(b.name()));

        Ok(DatabaseSchema {
            name: pattern.to_string(),
            db_type: DatabaseType::Elasticsearch,
            server_version: self.info.server_version.clone().unwrap_or_default(),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.request(Method::GET, "/", None)
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse console syntax: `METHOD /path` on the first line, JSON body after.
fn parse_console(text: &str) -> Option<(Method, String, &str)> {
    let text = text.trim_start();
    let (first, body) = text.split_once('\n').unwrap_or((text, ""));
    let mut parts = first.split_whitespace();
    let method = match parts.next()?.to_ascii_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "HEAD" => Method::HEAD,
        "PATCH" => Method::PATCH,
        _ => return None,
    };
    let path = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((method, format!("/{}", path.trim_start_matches('/')), body))
}

/// Whether a console request is side-effect free.
fn is_read_request(method: &Method, path: &str) -> bool {
    if *method == Method::GET || *method == Method::HEAD {
        return true;
    }
    if *method != Method::POST {
        return false;
    }
    let path = path.split(['?', '#']).next().unwrap_or_default();
    // `_sql/close` frees a cursor; everything else under `_sql` is a read.
    let segs: Vec<&str> = path.split('/').collect();
    // Dot segments / percent-encoding are normalised by the URL parser, so
    // `/_search/../logs/_update_by_query` would otherwise pass as a "_search".
    !path.contains('%')
        && segs
            .iter()
            .all(|s| !matches!(*s, "." | "..") && !WRITE_ENDPOINTS.contains(s))
        && segs.iter().any(|seg| READ_ENDPOINTS.contains(seg))
}

/// Turn a console response into a grid: search hits → one row per document,
/// arrays of objects (`_cat?format=json`) → table, anything else → one JSON cell.
fn response_to_result(resp: Json) -> QueryResult {
    let empty = |columns, rows| QueryResult {
        columns,
        rows,
        rows_affected: None,
        duration_ms: 0,
        explain_plan: None,
    };
    if let Some(hits) = resp["hits"]["hits"].as_array() {
        let docs: Vec<serde_json::Map<String, Json>> = hits
            .iter()
            .map(|h| {
                let mut m = serde_json::Map::new();
                m.insert("_id".into(), h["_id"].clone());
                m.insert("_index".into(), h["_index"].clone());
                if let Some(src) = h["_source"].as_object() {
                    m.extend(src.clone());
                }
                m
            })
            .collect();
        let (columns, rows) = rows_from_objects(&docs);
        return empty(columns, rows);
    }
    if let Some(arr) = resp.as_array() {
        if arr.iter().all(Json::is_object) {
            let objs: Vec<_> = arr.iter().filter_map(|v| v.as_object().cloned()).collect();
            let (columns, rows) = rows_from_objects(&objs);
            return empty(columns, rows);
        }
    }
    empty(
        vec![Column {
            name: "result".into(),
            col_type: ColumnType::Json,
            nullable: true,
            native_type: "json".into(),
        }],
        vec![Row {
            values: vec![json_to_value(resp)],
        }],
    )
}

/// Flatten mapping `properties` into dotted column paths.
fn flatten_mapping(props: &Json, prefix: &str, out: &mut Vec<ColumnSchema>) {
    let Some(map) = props.as_object() else { return };
    for (name, def) in map {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        if def.get("properties").is_some() {
            flatten_mapping(&def["properties"], &path, out);
            continue;
        }
        out.push(ColumnSchema {
            ordinal: out.len() as u32,
            name: path,
            native_type: def["type"].as_str().unwrap_or("object").to_string(),
            nullable: true,
            default_value: None,
            is_primary_key: false,
            is_unique: false,
            comment: None,
        });
    }
}

fn es_type(t: &str) -> ColumnType {
    match t {
        "long" | "integer" | "short" | "byte" | "unsigned_long" => ColumnType::Integer,
        "double" | "float" | "half_float" | "scaled_float" => ColumnType::Float,
        "boolean" => ColumnType::Boolean,
        "date" | "datetime" | "timestamp" | "date_nanos" => ColumnType::Timestamp,
        "object" | "nested" | "struct" => ColumnType::Object,
        other => json_type(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_parsing() {
        let (m, p, b) = parse_console("GET logs/_search\n{\"size\": 1}").unwrap();
        assert_eq!(
            (m, p.as_str(), b),
            (Method::GET, "/logs/_search", "{\"size\": 1}")
        );
        assert!(parse_console("SELECT * FROM logs").is_none());
        assert!(parse_console("GET /a extra").is_none());
    }

    #[test]
    fn read_only_classification() {
        assert!(is_read_request(&Method::GET, "/_cat/indices"));
        assert!(is_read_request(&Method::POST, "/logs/_search?size=5"));
        assert!(!is_read_request(&Method::POST, "/logs/_doc"));
        assert!(!is_read_request(&Method::POST, "/logs/_delete_by_query"));
        assert!(!is_read_request(&Method::DELETE, "/logs"));
        assert!(!is_read_request(&Method::PUT, "/logs/_search"));
        assert!(!is_read_request(
            &Method::POST,
            "/_search/../logs/_update_by_query"
        ));
        assert!(!is_read_request(
            &Method::POST,
            "/_search/%2e%2e/logs/_update_by_query"
        ));
        assert!(!is_read_request(
            &Method::POST,
            "/logs/_update_by_query#/_search"
        ));
        assert!(!is_read_request(&Method::POST, "/logs/_doc/_search"));
        assert!(is_read_request(&Method::POST, "/logs/_explain/1"));
    }
}
