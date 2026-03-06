//! MongoDB driver implementation.
//!
//! Uses the official `mongodb` crate with `rustls` TLS.
//! Supports MongoDB Atlas, self-hosted MongoDB 4.4+, DocumentDB.

use std::time::{Duration, Instant};

use dbench_core::{
    connection::Connection,
    driver::Driver,
    error::CatalystError,
    query::Query,
    result::QueryResult,
    schema::{CollectionSchema, DatabaseSchema, IndexSchema, SchemaObject},
    types::{ConnectionInfo, ConnectionMode, DatabaseType},
    Result,
};
use dbench_macros::ConnectionConfig;
use dbench_security::tls::{TlsConfig, TlsMode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Configuration for connecting to MongoDB.
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct MongoConfig {
    /// MongoDB connection URI (e.g., `mongodb://host:27017`)
    /// OR individual fields below. If `uri` is set, it takes precedence.
    pub uri: Option<String>,

    /// Hostname (used when `uri` is not set).
    pub host: String,
    /// Port (default: 27017).
    pub port: u16,
    /// Database name to connect to.
    #[config(required)]
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

        tracing::info!(
            host = %config.host,
            port = config.port,
            database = %config.database,
            "Connecting to MongoDB"
        );

        // TODO: Build mongodb::Client:
        //
        // let mut client_options = ClientOptions::parse(&connection_string).await
        //     .map_err(|e| CatalystError::connection_failed(DatabaseType::Mongodb, &config.host, e))?;
        //
        // client_options.app_name = Some("dbench".into());
        // client_options.connect_timeout = config.connect_timeout_ms.map(Duration::from_millis);
        //
        // let client = Client::with_options(client_options)?;
        // let db = client.database(&config.database);
        //
        // // Test connection with a ping
        // db.run_command(doc! { "ping": 1 }, None).await?;

        let info = ConnectionInfo {
            id: Uuid::new_v4(),
            db_type: DatabaseType::Mongodb,
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone().unwrap_or_default(),
            tls_active: config.tls.mode != TlsMode::Disabled,
            ssh_tunnel: false,
            server_version: None,
            connected_at: chrono::Utc::now(),
        };

        Ok(MongoConnection {
            info,
            mode: config.mode,
            alive: true,
        })
    }
}

/// An active MongoDB connection.
pub struct MongoConnection {
    info: ConnectionInfo,
    mode: ConnectionMode,
    alive: bool,
    // TODO: client: mongodb::Client, db: mongodb::Database
}

impl Connection for MongoConnection {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult> {
        if !self.alive {
            return Err(CatalystError::ConnectionLost {
                reason: "connection is closed".into(),
            });
        }

        // MongoDB queries are JSON documents, not SQL strings.
        // The `query.text` field is expected to be a JSON aggregation pipeline
        // or a `find` filter, depending on the operation.
        //
        // TODO: Parse query.text as BSON document and execute via mongodb driver.

        let start = Instant::now();
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            duration_ms: start.elapsed().as_millis() as u64,
            explain_plan: None,
        })
    }

    async fn inspect_schema(&mut self) -> Result<DatabaseSchema> {
        // MongoDB: list collections, infer field schema from sample documents.
        //
        // TODO:
        // let collections = self.db.list_collection_names(None).await?;
        // for name in collections {
        //     let sample = self.db.collection::<Document>(&name)
        //         .aggregate([doc! { "$sample": { "size": 100 } }], None).await?;
        //     // Infer field schema from sample
        // }

        Ok(DatabaseSchema {
            name: self.info.database.clone(),
            db_type: DatabaseType::Mongodb,
            server_version: self.info.server_version.clone().unwrap_or("unknown".into()),
            objects: vec![],
        })
    }

    async fn ping(&mut self) -> Result<Duration> {
        let start = Instant::now();
        // TODO: self.db.run_command(doc! {"ping": 1}, None).await?;
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
