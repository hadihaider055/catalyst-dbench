//! Amazon DynamoDB driver via the AWS SDK. Queries are PartiQL (`ExecuteStatement`).
//!
//! Credentials: explicit access key/secret if given, otherwise the standard AWS chain
//! (env vars, `~/.aws` profiles incl. SSO, instance/container roles).

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use aws_sdk_dynamodb::{
    config::{BehaviorVersion, Credentials, Region},
    types::AttributeValue,
    Client,
};
use dbench_core::{
    connection::Connection,
    driver::{ConnectionConfig, Driver},
    error::CatalystError,
    guard::strip_comments_and_strings,
    query::Query,
    result::QueryResult,
    schema::{ColumnSchema, DatabaseSchema, IndexSchema, SchemaObject, TableSchema},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
use uuid::Uuid;

use crate::http::rows_from_objects;

/// Stop paging after this many items so a full-table scan can't exhaust memory.
const MAX_ROWS: usize = 5_000;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for DynamoDB.
#[derive(ConnectionConfig, Clone, Serialize, Deserialize)]
pub struct DynamoConfig {
    #[config(required)]
    pub region: String,
    /// Custom endpoint, e.g. `http://localhost:8000` for DynamoDB Local.
    pub endpoint: Option<String>,
    /// Access key ID; empty = default credential chain.
    pub access_key_id: String,
    #[config(secret)]
    pub secret_access_key: Option<String>,
    pub mode: ConnectionMode,
}

impl Default for DynamoConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".into(),
            endpoint: None,
            access_key_id: String::new(),
            secret_access_key: None,
            mode: ConnectionMode::ReadWrite,
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// DynamoDB driver.
#[derive(Debug, Default)]
pub struct DynamoDriver;

impl Driver for DynamoDriver {
    type Connection = DynamoConnection;
    type Config = DynamoConfig;

    fn name(&self) -> &'static str {
        "dynamodb"
    }
    fn database_type(&self) -> DatabaseType {
        DatabaseType::Dynamodb
    }

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
        config.validate()?;
        let endpoint = config.endpoint.clone().filter(|e| !e.is_empty());
        let target = endpoint
            .clone()
            .unwrap_or_else(|| format!("dynamodb.{}.amazonaws.com", config.region));
        let fail = |e: String| CatalystError::connection_failed(DatabaseType::Dynamodb, &target, e);

        // Plain HTTP is only acceptable for a local emulator.
        if let Some(ep) = &endpoint {
            let url = reqwest::Url::parse(ep)
                .map_err(|e| CatalystError::Config(format!("invalid endpoint: {e}")))?;
            let loopback = matches!(
                url.host_str(),
                Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
            );
            if url.scheme() != "https" && !loopback {
                return Err(CatalystError::Config(
                    "non-local DynamoDB endpoints must use https://".into(),
                ));
            }
        }

        let mut loader = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()));
        if !config.access_key_id.is_empty() {
            loader = loader.credentials_provider(Credentials::new(
                &config.access_key_id,
                config.secret_access_key.clone().unwrap_or_default(),
                None,
                None,
                "catalyst-dbench",
            ));
        }
        if let Some(ep) = &endpoint {
            loader = loader.endpoint_url(ep);
        }
        let client = Client::new(&loader.load().await);

        // Verifies credentials + reachability.
        client
            .list_tables()
            .limit(1)
            .send()
            .await
            .map_err(|e| fail(sdk_error(&e)))?;

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Dynamodb,
            host: target.clone(),
            port: 443,
            database: config.region.clone(),
            username: config.access_key_id.clone(),
            tls_active: endpoint.as_deref().is_none_or(|e| e.starts_with("https")),
            ssh_tunnel: false,
            server_version: Some("DynamoDB".into()),
            connected_at: chrono::Utc::now(),
        };
        tracing::info!(conn_id = %info.id, "DynamoDB connection established");
        Ok(DynamoConnection {
            info,
            mode: config.mode,
            client: Some(client),
        })
    }
}

/// Unwrap the service error message (the default Display is just "service error").
fn sdk_error<E: std::error::Error + 'static>(e: &E) -> String {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg = s.to_string();
        src = s.source();
    }
    msg
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// A live DynamoDB client.
pub struct DynamoConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    client: Option<Client>,
}

impl DynamoConnection {
    fn client(&self) -> Result<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            })
    }
}

/// PartiQL `ExecuteStatement` runs exactly one statement; only `SELECT` reads.
fn is_partiql_read(text: &str) -> bool {
    strip_comments_and_strings(text)
        .trim_start()
        .get(..6)
        .is_some_and(|w| w.eq_ignore_ascii_case("SELECT"))
}

impl Connection for DynamoConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.mode.allows_writes() && !is_partiql_read(&query.text) {
            return Err(CatalystError::ReadOnlyViolation);
        }
        let start = Instant::now();
        let statement = query.text.trim().trim_end_matches(';').to_string();
        let mut items: Vec<Map<String, Json>> = Vec::new();
        let mut next: Option<String> = None;
        loop {
            let resp = self
                .client()?
                .execute_statement()
                .statement(&statement)
                .set_next_token(next.take())
                .send()
                .await
                .map_err(|e| CatalystError::query_failed(sdk_error(&e)))?;
            items.extend(resp.items().iter().map(item_to_json));
            next = resp.next_token().map(str::to_owned);
            if next.is_none() || items.len() >= MAX_ROWS {
                break;
            }
        }
        items.truncate(MAX_ROWS);
        let (columns, rows) = rows_from_objects(&items);
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            duration_ms: start.elapsed().as_millis() as u64,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        let client = self.client()?;
        let err = |e: String| CatalystError::SchemaError(e);
        let mut names = Vec::new();
        let mut start: Option<String> = None;
        loop {
            let resp = client
                .list_tables()
                .set_exclusive_start_table_name(start.take())
                .send()
                .await
                .map_err(|e| err(sdk_error(&e)))?;
            names.extend(resp.table_names().iter().cloned());
            start = resp.last_evaluated_table_name().map(str::to_owned);
            if start.is_none() {
                break;
            }
        }

        let mut objects = Vec::new();
        for name in names {
            let desc = client
                .describe_table()
                .table_name(&name)
                .send()
                .await
                .map_err(|e| err(sdk_error(&e)))?;
            let Some(t) = desc.table() else { continue };
            let attr_types: HashMap<&str, String> = t
                .attribute_definitions()
                .iter()
                .map(|a| (a.attribute_name(), a.attribute_type().as_str().to_string()))
                .collect();
            let key_cols: Vec<String> = t
                .key_schema()
                .iter()
                .map(|k| k.attribute_name().to_string())
                .collect();
            let columns = t
                .attribute_definitions()
                .iter()
                .enumerate()
                .map(|(i, a)| ColumnSchema {
                    name: a.attribute_name().to_string(),
                    ordinal: i as u32 + 1,
                    native_type: attr_types
                        .get(a.attribute_name())
                        .cloned()
                        .unwrap_or_default(),
                    nullable: !key_cols.iter().any(|k| k == a.attribute_name()),
                    default_value: None,
                    is_primary_key: key_cols.iter().any(|k| k == a.attribute_name()),
                    is_unique: false,
                    comment: None,
                })
                .collect();
            let mut indexes = vec![IndexSchema {
                name: "PRIMARY".into(),
                columns: key_cols,
                is_unique: true,
                is_primary: true,
                index_type: "hash/range".into(),
            }];
            for g in t.global_secondary_indexes() {
                indexes.push(IndexSchema {
                    name: g.index_name().unwrap_or_default().to_string(),
                    columns: g
                        .key_schema()
                        .iter()
                        .map(|k| k.attribute_name().to_string())
                        .collect(),
                    is_unique: false,
                    is_primary: false,
                    index_type: "gsi".into(),
                });
            }
            for l in t.local_secondary_indexes() {
                indexes.push(IndexSchema {
                    name: l.index_name().unwrap_or_default().to_string(),
                    columns: l
                        .key_schema()
                        .iter()
                        .map(|k| k.attribute_name().to_string())
                        .collect(),
                    is_unique: false,
                    is_primary: false,
                    index_type: "lsi".into(),
                });
            }
            objects.push(SchemaObject::Table(TableSchema {
                schema: None,
                name,
                columns,
                indexes,
                foreign_keys: vec![],
                row_count: t.item_count().and_then(|n| u64::try_from(n).ok()),
                comment: None,
            }));
        }

        Ok(DatabaseSchema {
            name: self.info.database.clone(),
            db_type: DatabaseType::Dynamodb,
            server_version: "DynamoDB".into(),
            objects,
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        self.client()?
            .list_tables()
            .limit(1)
            .send()
            .await
            .map_err(|e| CatalystError::ConnectionLost {
                reason: sdk_error(&e),
            })?;
        Ok(start.elapsed())
    }

    async fn close(mut self) -> Result<()> {
        self.client = None;
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
// AttributeValue → JSON
// ---------------------------------------------------------------------------

fn item_to_json(item: &HashMap<String, AttributeValue>) -> Map<String, Json> {
    item.iter()
        .map(|(k, v)| (k.clone(), attr_to_json(v)))
        .collect()
}

fn number(n: &str) -> Json {
    n.parse::<i64>()
        .map(Json::from)
        .or_else(|_| n.parse::<f64>().map(Json::from))
        .unwrap_or_else(|_| Json::String(n.to_string()))
}

fn attr_to_json(v: &AttributeValue) -> Json {
    match v {
        AttributeValue::S(s) => Json::String(s.clone()),
        AttributeValue::N(n) => number(n),
        AttributeValue::Bool(b) => Json::Bool(*b),
        AttributeValue::Null(_) => Json::Null,
        AttributeValue::B(b) => Json::String(format!("<binary {} bytes>", b.as_ref().len())),
        AttributeValue::L(l) => Json::Array(l.iter().map(attr_to_json).collect()),
        AttributeValue::M(m) => Json::Object(item_to_json(m)),
        AttributeValue::Ss(s) => Json::Array(s.iter().cloned().map(Json::String).collect()),
        AttributeValue::Ns(n) => Json::Array(n.iter().map(|x| number(x)).collect()),
        AttributeValue::Bs(b) => Json::Array(
            b.iter()
                .map(|x| Json::String(format!("<binary {} bytes>", x.as_ref().len())))
                .collect(),
        ),
        _ => Json::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partiql_read_only() {
        assert!(is_partiql_read("  select * from \"Orders\""));
        assert!(is_partiql_read("-- note\nSELECT * FROM t"));
        assert!(!is_partiql_read("UPDATE t SET a = 1 WHERE pk = 'x'"));
        assert!(!is_partiql_read(
            "/* SELECT */ DELETE FROM t WHERE pk = 'x'"
        ));
    }

    #[test]
    fn attribute_conversion() {
        assert_eq!(
            attr_to_json(&AttributeValue::N("42".into())),
            Json::from(42)
        );
        assert_eq!(
            attr_to_json(&AttributeValue::N("1.5".into())),
            Json::from(1.5)
        );
        let m = AttributeValue::M(HashMap::from([(
            "k".to_string(),
            AttributeValue::Bool(true),
        )]));
        assert_eq!(attr_to_json(&m)["k"], Json::Bool(true));
    }
}
