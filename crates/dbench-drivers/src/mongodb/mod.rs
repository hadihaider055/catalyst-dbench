//! MongoDB driver implementation.
//!
//! Uses the official `mongodb` crate with `rustls` TLS.
//! Supports MongoDB Atlas, self-hosted MongoDB 4.4+, DocumentDB.

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    query::Query,
    result::{Column, ColumnType, QueryResult, Row, Value},
    schema::{CollectionSchema, DatabaseSchema, IndexSchema, InferredField, SchemaObject},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Bson, Document},
    options::{
        ClientOptions, Credential, FindOptions, ServerApi, ServerApiVersion, Tls, TlsOptions,
    },
    Client,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for connecting to MongoDB.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct MongoConfig {
    /// MongoDB connection URI (e.g., `mongodb://host:27017`).
    /// If set, it takes precedence over individual fields. May embed credentials.
    #[config(secret)]
    pub uri: Option<String>,
    /// Hostname (used when `uri` is not set).
    pub host: String,
    /// Port (default: 27017).
    pub port: u16,
    /// Database name. Required when not using a URI; optional when uri is set
    /// (the URI's path component is used as the database name in that case).
    pub database: String,
    /// Username (if auth enabled).
    pub username: Option<String>,
    /// Password — stored in OS keychain.
    #[config(secret)]
    pub password: Option<String>,
    /// Auth source database (default: `admin`).
    pub auth_source: Option<String>,
    /// Replica set name.
    pub replica_set: Option<String>,
    /// TLS configuration.
    pub tls: TlsConfig,
    /// Connection mode.
    pub mode: ConnectionMode,
    /// Connection timeout in milliseconds.
    pub connect_timeout_ms: Option<u64>,
}

impl Default for MongoConfig {
    fn default() -> Self {
        Self {
            uri: None,
            host: "localhost".into(),
            port: 27017,
            database: String::new(),
            username: None,
            password: None,
            auth_source: None,
            replica_set: None,
            tls: TlsConfig::required(),
            mode: ConnectionMode::ReadWrite,
            connect_timeout_ms: Some(10_000),
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// MongoDB driver.
#[derive(Debug, Default)]
pub struct MongoDriver;

impl Driver for MongoDriver {
    type Connection = MongoConnection;
    type Config = MongoConfig;

    fn name(&self) -> &'static str {
        "mongodb"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Mongodb
    }
    fn default_port(&self) -> Option<u16> {
        Some(27017)
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        config.tls.validate()?;

        let conn_str = if let Some(uri) = &config.uri {
            uri.clone()
        } else {
            if config.database.is_empty() {
                return Err(CatalystError::Config("database name is required".into()));
            }
            // Credentials go in `ClientOptions::credential` below, never into the URI
            // string, so passwords containing `@`, `:` or `/` can't change the target host.
            format!(
                "mongodb://{}:{}/{}",
                config.host, config.port, config.database
            )
        };

        let timeout = Duration::from_millis(config.connect_timeout_ms.unwrap_or(10_000));

        let mut client_options = ClientOptions::parse(&conn_str).await.map_err(|e| {
            CatalystError::connection_failed(DatabaseType::Mongodb, &config.host, e.to_string())
        })?;

        // Resolve the database name: explicit config > URI path component > "admin" (default auth db).
        // We always need *some* database to send the initial ping, but inspect_schema will
        // list all databases regardless of which one we connected to.
        let db_name = if !config.database.is_empty() {
            config.database.clone()
        } else {
            client_options
                .default_database
                .clone()
                .unwrap_or_else(|| "admin".into())
        };

        client_options.app_name = Some("catalyst-dbench".into());
        client_options.connect_timeout = Some(timeout);
        client_options.server_selection_timeout = Some(timeout);
        client_options.server_api = Some(
            ServerApi::builder()
                .version(ServerApiVersion::V1)
                .strict(false)
                .build(),
        );

        if let Some(rs) = &config.replica_set {
            client_options.repl_set_name = Some(rs.clone());
        }

        if config.uri.is_none() {
            if let Some(user) = &config.username {
                client_options.credential = Some(
                    Credential::builder()
                        .username(user.clone())
                        .password(config.password.clone())
                        .source(config.auth_source.clone().unwrap_or_else(|| "admin".into()))
                        .build(),
                );
            }
        }

        // Honour the TLS setting (a URI's own `tls=` option still applies when this is off).
        if config.tls.mode != TlsMode::Disabled {
            client_options.tls = Some(Tls::Enabled(
                TlsOptions::builder()
                    .ca_file_path(config.tls.ca_cert_path.clone())
                    .build(),
            ));
        }

        tracing::info!(
            db_name = %db_name,
            "Connecting to MongoDB"
        );

        let client = Client::with_options(client_options).map_err(|e| {
            CatalystError::connection_failed(DatabaseType::Mongodb, &config.host, e.to_string())
        })?;

        let db = client.database(&db_name);

        db.run_command(doc! { "ping": 1 }).await.map_err(|e| {
            CatalystError::connection_failed(DatabaseType::Mongodb, &config.host, e.to_string())
        })?;

        let server_version = db
            .run_command(doc! { "buildInfo": 1 })
            .await
            .ok()
            .and_then(|d| d.get_str("version").ok().map(String::from));

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Mongodb,
            host: config.host.clone(),
            port: config.port,
            database: db_name.clone(),
            username: config.username.clone().unwrap_or_default(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version,
            connected_at: chrono::Utc::now(),
        };

        tracing::info!(conn_id = %info.id, "MongoDB connection established");

        Ok(MongoConnection {
            info,
            mode: config.mode,
            alive: true,
            client,
            db_name,
        })
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// An active MongoDB connection.
pub struct MongoConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    client: Client,
    db_name: String,
}

impl Connection for MongoConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }

        let start = Instant::now();
        let db = self.client.database(&self.db_name);

        // Parse query text as JSON. Supported formats:
        //   {"find": "collection", "filter": {...}, "limit": 100}
        //   {"aggregate": "collection", "pipeline": [...]}
        //   {"insert": "collection", "documents": [...]}
        //   {"command": {...}}  — raw db command
        let parsed: serde_json::Value = serde_json::from_str(&query.text)
            .map_err(|e| CatalystError::query_failed(format!("Invalid query JSON: {e}")))?;

        let obj = parsed
            .as_object()
            .ok_or_else(|| CatalystError::query_failed("Query must be a JSON object"))?;

        // Write guard.
        if !self.mode.allows_writes() && is_mongo_write(obj) {
            return Err(CatalystError::ReadOnlyViolation);
        }

        // Allow callers to target a specific database for this operation.
        let db = if let Some(db_override) = obj.get("db").and_then(|v| v.as_str()) {
            self.client.database(db_override)
        } else {
            db
        };

        // If explain mode: wrap the operation in MongoDB's explain command and
        // return the plan as explain_plan text (not as rows).
        if query.explain {
            // Build the inner command doc by stripping the "db" key
            // (it's our routing field, not part of the MongoDB protocol).
            let mut inner = obj.clone();
            inner.remove("db");
            let inner_doc: Document =
                serde_json::from_value(serde_json::Value::Object(inner.clone()))
                    .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            // Only find and aggregate support explain natively.
            let explainable = inner.contains_key("find") || inner.contains_key("aggregate");
            if !explainable {
                let plan = format!(
                    "explain is only supported for find and aggregate operations.\nOperation: {}",
                    serde_json::to_string_pretty(&serde_json::Value::Object(inner))
                        .unwrap_or_default()
                );
                return Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                    rows_affected: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    explain_plan: Some(plan),
                });
            }

            let explain_cmd = doc! {
                "explain": inner_doc,
                "verbosity": "executionStats",
            };
            let result_doc = db
                .run_command(explain_cmd)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            let plan = serde_json::to_string_pretty(
                &serde_json::to_value(&result_doc).unwrap_or(serde_json::Value::Null),
            )
            .unwrap_or_else(|_| result_doc.to_string());

            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: Some(plan),
            });
        }

        // Handle "find".
        if let Some(coll_name) = obj.get("find").and_then(|v| v.as_str()) {
            let collection = db.collection::<Document>(coll_name);
            let filter: Option<Document> = obj
                .get("filter")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let limit = obj.get("limit").and_then(|v| v.as_i64());
            let skip = obj.get("skip").and_then(|v| v.as_u64());
            let sort: Option<Document> = obj
                .get("sort")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let projection: Option<Document> = obj
                .get("projection")
                .and_then(|v| serde_json::from_value(v.clone()).ok());

            let mut options = FindOptions::default();
            options.limit = limit;
            options.skip = skip;
            options.sort = sort;
            options.projection = projection;

            let cursor = collection
                .find(filter.unwrap_or_default())
                .with_options(options)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            let docs: Vec<Document> = cursor
                .try_collect()
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            return Ok(docs_to_result(docs, start.elapsed().as_millis() as u64));
        }

        // Handle "aggregate".
        if let Some(coll_name) = obj.get("aggregate").and_then(|v| v.as_str()) {
            let collection = db.collection::<Document>(coll_name);
            let pipeline: Vec<Document> = obj
                .get("pipeline")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let cursor = collection
                .aggregate(pipeline)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            let docs: Vec<Document> = cursor
                .try_collect()
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;

            return Ok(docs_to_result(docs, start.elapsed().as_millis() as u64));
        }

        // Handle "insert".
        if let Some(coll_name) = obj.get("insert").and_then(|v| v.as_str()) {
            let collection = db.collection::<Document>(coll_name);
            let documents: Vec<Document> = obj
                .get("documents")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let count = documents.len() as u64;
            collection
                .insert_many(documents)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: Some(count),
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            });
        }

        // Handle "update".
        if let Some(coll_name) = obj.get("update").and_then(|v| v.as_str()) {
            let updates = obj
                .get("updates")
                .and_then(|v| v.as_array())
                .ok_or_else(|| CatalystError::query_failed("update requires 'updates' array"))?;

            let mut total_modified = 0u64;
            for update_spec in updates {
                let q: Document = coerce_objectids(
                    serde_json::from_value(
                        update_spec
                            .get("q")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                    )
                    .map_err(|e| CatalystError::query_failed(e.to_string()))?,
                );
                let u: Document = serde_json::from_value(
                    update_spec
                        .get("u")
                        .cloned()
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                )
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;
                let multi = update_spec
                    .get("multi")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let collection = db.collection::<Document>(coll_name);
                if multi {
                    let res = collection
                        .update_many(q, u)
                        .await
                        .map_err(|e| CatalystError::query_failed(e.to_string()))?;
                    total_modified += res.modified_count;
                } else {
                    let res = collection
                        .update_one(q, u)
                        .await
                        .map_err(|e| CatalystError::query_failed(e.to_string()))?;
                    total_modified += res.modified_count;
                }
            }
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: Some(total_modified),
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            });
        }

        // Handle "delete".
        if let Some(coll_name) = obj.get("delete").and_then(|v| v.as_str()) {
            let deletes = obj
                .get("deletes")
                .and_then(|v| v.as_array())
                .ok_or_else(|| CatalystError::query_failed("delete requires 'deletes' array"))?;

            let mut total_deleted = 0u64;
            for delete_spec in deletes {
                let q: Document = coerce_objectids(
                    serde_json::from_value(
                        delete_spec
                            .get("q")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                    )
                    .map_err(|e| CatalystError::query_failed(e.to_string()))?,
                );
                let limit = delete_spec
                    .get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);

                let collection = db.collection::<Document>(coll_name);
                if limit == 0 {
                    let res = collection
                        .delete_many(q)
                        .await
                        .map_err(|e| CatalystError::query_failed(e.to_string()))?;
                    total_deleted += res.deleted_count;
                } else {
                    let res = collection
                        .delete_one(q)
                        .await
                        .map_err(|e| CatalystError::query_failed(e.to_string()))?;
                    total_deleted += res.deleted_count;
                }
            }
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: Some(total_deleted),
                duration_ms: start.elapsed().as_millis() as u64,
                explain_plan: None,
            });
        }

        // Handle "command".
        if let Some(cmd_val) = obj.get("command") {
            let cmd: Document = serde_json::from_value(cmd_val.clone())
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;
            let result_doc = db
                .run_command(cmd)
                .await
                .map_err(|e| CatalystError::query_failed(e.to_string()))?;
            return Ok(docs_to_result(
                vec![result_doc],
                start.elapsed().as_millis() as u64,
            ));
        }

        Err(CatalystError::query_failed(
            "Unsupported operation. Use: {\"find\": \"col\", ...}, {\"aggregate\": \"col\", \"pipeline\": [...]}, {\"update\": \"col\", \"updates\": [...]}, {\"delete\": \"col\", \"deletes\": [...]}, or {\"command\": {...}}"
        ))
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        // List all user-accessible databases, skipping internal ones the user can't use.
        let db_names = self
            .client
            .list_database_names()
            .await
            .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

        let system_dbs = ["admin", "config", "local"];
        let user_dbs: Vec<String> = db_names
            .into_iter()
            .filter(|n| !system_dbs.contains(&n.as_str()))
            .collect();

        let mut objects = Vec::new();

        for db_name in &user_dbs {
            let db = self.client.database(db_name);

            let collection_names = db.list_collection_names().await.unwrap_or_default();

            for coll_name in collection_names {
                let collection = db.collection::<Document>(&coll_name);

                let pipeline = vec![doc! { "$sample": { "size": 50 } }];
                let samples: Vec<Document> = match collection.aggregate(pipeline).await {
                    Ok(cursor) => cursor.try_collect().await.unwrap_or_default(),
                    Err(_) => vec![],
                };

                let mut field_types: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();
                for doc in &samples {
                    collect_fields(doc, "", &mut field_types);
                }

                let total_count = db
                    .run_command(doc! { "collStats": &coll_name })
                    .await
                    .ok()
                    .and_then(|d| d.get_i64("count").ok())
                    .map(|c| c as u64);

                let index_cursor = collection.list_indexes().await.ok();
                let indexes: Vec<IndexSchema> = if let Some(cursor) = index_cursor {
                    cursor
                        .try_collect::<Vec<_>>()
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .map(|idx| {
                            let columns: Vec<String> = idx.keys.keys().cloned().collect();
                            let is_unique =
                                idx.options.as_ref().and_then(|o| o.unique).unwrap_or(false);
                            let idx_name = idx
                                .options
                                .as_ref()
                                .and_then(|o| o.name.clone())
                                .unwrap_or_else(|| columns.join("_"));
                            let is_primary = idx_name == "_id_";
                            IndexSchema {
                                name: idx_name,
                                columns,
                                is_unique,
                                is_primary,
                                index_type: "btree".into(),
                            }
                        })
                        .collect()
                } else {
                    vec![]
                };

                let inferred_fields: Vec<InferredField> = field_types
                    .into_iter()
                    .map(|(path, inferred_type)| InferredField {
                        occurrence_rate: if samples.is_empty() { 0.0 } else { 1.0 },
                        path,
                        inferred_type,
                    })
                    .collect();

                objects.push(SchemaObject::Collection(CollectionSchema {
                    database: Some(db_name.clone()),
                    name: coll_name,
                    inferred_fields,
                    indexes,
                    document_count: total_count,
                    size_bytes: None,
                }));
            }
        }

        Ok(DatabaseSchema {
            name: self.db_name.clone(),
            db_type: DatabaseType::Mongodb,
            server_version: self
                .info
                .server_version
                .clone()
                .unwrap_or_else(|| "MongoDB".into()),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        let db = self.client.database(&self.db_name);
        db.run_command(doc! { "ping": 1 })
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

/// Coerce string values in a filter document to ObjectId where possible.
/// Needed because _id is stored as ObjectId but the frontend sends back a hex string.
/// Raw `{"command": {...}}` names allowed on read-only connections.
const READ_COMMANDS: &[&str] = &[
    "find",
    "aggregate",
    "count",
    "distinct",
    "listCollections",
    "listIndexes",
    "listDatabases",
    "dbStats",
    "collStats",
    "serverStatus",
    "buildInfo",
    "hostInfo",
    "ping",
    "connectionStatus",
];

/// Whether a query object could modify data: a write operation, an aggregation
/// with a `$out`/`$merge` stage, or a raw command that isn't a known read.
fn is_mongo_write(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    fn writes_stage(pipeline: Option<&serde_json::Value>) -> bool {
        pipeline.and_then(|p| p.as_array()).is_some_and(|stages| {
            stages.iter().any(|s| {
                s.as_object()
                    .is_some_and(|o| o.contains_key("$out") || o.contains_key("$merge"))
            })
        })
    }
    const WRITE_OPS: &[&str] = &["insert", "update", "delete", "drop", "create", "rename"];
    if WRITE_OPS.iter().any(|op| obj.contains_key(*op)) || writes_stage(obj.get("pipeline")) {
        return true;
    }
    match obj.get("command") {
        // The server treats the first key as the command name.
        Some(serde_json::Value::Object(cmd)) => {
            !cmd.keys()
                .next()
                .is_some_and(|name| READ_COMMANDS.contains(&name.as_str()))
                || writes_stage(cmd.get("pipeline"))
        }
        Some(_) => true,
        None => false,
    }
}

fn coerce_objectids(doc: Document) -> Document {
    doc.into_iter()
        .map(|(k, v)| {
            let v = match v {
                Bson::String(ref s) => {
                    if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(s) {
                        Bson::ObjectId(oid)
                    } else {
                        v
                    }
                }
                other => other,
            };
            (k, v)
        })
        .collect()
}

fn bson_type_name(bson: &Bson) -> &'static str {
    match bson {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Array(_) => "array",
        Bson::Document(_) => "document",
        Bson::Boolean(_) => "boolean",
        Bson::Null => "null",
        Bson::RegularExpression(_) => "regex",
        Bson::JavaScriptCode(_) | Bson::JavaScriptCodeWithScope(_) => "javascript",
        Bson::Int32(_) | Bson::Int64(_) => "int",
        Bson::Timestamp(_) | Bson::DateTime(_) => "date",
        Bson::Binary(_) => "binary",
        Bson::ObjectId(_) => "objectId",
        Bson::Decimal128(_) => "decimal",
        _ => "other",
    }
}

fn collect_fields(
    doc: &Document,
    prefix: &str,
    out: &mut std::collections::HashMap<String, String>,
) {
    for (key, val) in doc {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match val {
            Bson::Document(nested) => {
                out.entry(path.clone()).or_insert_with(|| "document".into());
                collect_fields(nested, &path, out);
            }
            other => {
                out.entry(path)
                    .or_insert_with(|| bson_type_name(other).into());
            }
        }
    }
}

fn bson_to_value(bson: Bson) -> Value {
    match bson {
        Bson::Null | Bson::Undefined => Value::Null,
        Bson::Boolean(b) => Value::Bool(b),
        Bson::Int32(i) => Value::Int(i64::from(i)),
        Bson::Int64(i) => Value::Int(i),
        Bson::Double(f) => Value::Float(f),
        Bson::Decimal128(d) => Value::Decimal(d.to_string()),
        Bson::String(s) => Value::Text(s),
        Bson::ObjectId(oid) => Value::Text(oid.to_hex()),
        Bson::DateTime(dt) => {
            let millis = dt.timestamp_millis();
            use chrono::TimeZone;
            let ts = chrono::Utc
                .timestamp_millis_opt(millis)
                .single()
                .unwrap_or_else(chrono::Utc::now);
            Value::Timestamp(ts)
        }
        Bson::Timestamp(ts) => Value::Int(i64::from(ts.time)),
        Bson::Binary(bin) => Value::Bytes(bin.bytes),
        Bson::Array(arr) => Value::Array(arr.into_iter().map(bson_to_value).collect()),
        Bson::Document(doc) => {
            let json = serde_json::to_value(doc).unwrap_or(serde_json::Value::Null);
            match json {
                serde_json::Value::Object(m) => Value::Object(m),
                other => Value::Json(other),
            }
        }
        Bson::RegularExpression(re) => Value::Text(format!("/{}/{}", re.pattern, re.options)),
        Bson::JavaScriptCode(js) => Value::Text(js),
        Bson::JavaScriptCodeWithScope(scope) => Value::Text(scope.code),
        _ => Value::Text(bson.to_string()),
    }
}

fn docs_to_result(docs: Vec<Document>, duration_ms: u64) -> QueryResult {
    if docs.is_empty() {
        return QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            duration_ms,
            explain_plan: None,
        };
    }

    // Union schema from all documents.
    let mut field_order: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for doc in &docs {
        for key in doc.keys() {
            if seen.insert(key.clone()) {
                field_order.push(key.clone());
            }
        }
    }

    // Infer native_type from the first non-null value in each column.
    let columns: Vec<Column> = field_order
        .iter()
        .map(|name| {
            let native_type = docs
                .iter()
                .find_map(|doc| doc.get(name))
                .map(|bson| match bson {
                    Bson::ObjectId(_) => "ObjectId",
                    Bson::String(_) => "String",
                    Bson::Int32(_) => "Int32",
                    Bson::Int64(_) => "Int64",
                    Bson::Double(_) => "Double",
                    Bson::Decimal128(_) => "Decimal128",
                    Bson::Boolean(_) => "Boolean",
                    Bson::DateTime(_) => "DateTime",
                    Bson::Timestamp(_) => "Timestamp",
                    Bson::Array(_) => "Array",
                    Bson::Document(_) => "Object",
                    Bson::Binary(_) => "Binary",
                    Bson::Null | Bson::Undefined => "null",
                    _ => "Mixed",
                })
                .unwrap_or("Mixed");
            let col_type = match native_type {
                "Int32" | "Int64" => ColumnType::Integer,
                "Double" | "Decimal128" => ColumnType::Float,
                "Boolean" => ColumnType::Boolean,
                "DateTime" | "Timestamp" => ColumnType::Timestamp,
                "Binary" => ColumnType::Bytes,
                _ => ColumnType::Text,
            };
            Column {
                name: name.clone(),
                col_type,
                nullable: true,
                native_type: native_type.into(),
            }
        })
        .collect();

    let rows: Vec<Row> = docs
        .into_iter()
        .map(|mut doc| {
            let values = field_order
                .iter()
                .map(|field| doc.remove(field).map(bson_to_value).unwrap_or(Value::Null))
                .collect();
            Row { values }
        })
        .collect();

    QueryResult {
        columns,
        rows,
        rows_affected: None,
        duration_ms,
        explain_plan: None,
    }
}

#[cfg(test)]
mod tests {
    use super::is_mongo_write;

    fn write(q: &str) -> bool {
        let v: serde_json::Value = serde_json::from_str(q).unwrap();
        is_mongo_write(v.as_object().unwrap())
    }

    #[test]
    fn read_only_guard_catches_indirect_writes() {
        assert!(!write(r#"{"find":"users","filter":{}}"#));
        assert!(!write(
            r#"{"aggregate":"users","pipeline":[{"$match":{}}]}"#
        ));
        assert!(!write(r#"{"command":{"listDatabases":1},"db":"admin"}"#));
        assert!(write(r#"{"delete":"users","deletes":[]}"#));
        assert!(write(
            r#"{"aggregate":"users","pipeline":[{"$out":"copy"}]}"#
        ));
        assert!(write(
            r#"{"aggregate":"users","pipeline":[{"$merge":{"into":"c"}}]}"#
        ));
        assert!(write(r#"{"command":{"dropDatabase":1}}"#));
        assert!(write(
            r#"{"command":{"findAndModify":"users","remove":true}}"#
        ));
        assert!(write(
            r#"{"command":{"aggregate":"users","pipeline":[{"$out":"c"}],"cursor":{}}}"#
        ));
        assert!(write(r#"{"command":"dropDatabase"}"#));
    }
}
