//! The `Driver` trait — the entry point for every database type.

use crate::{connection::Connection, types::ConnectionInfo, Result};

/// A database driver. One implementation exists per database type
/// (PostgreSQL, MySQL, MongoDB, etc.).
///
/// A `Driver` is stateless and cheap to clone. It describes **how** to connect
/// to a database type; the [`Connection`] it produces represents **a live connection**.
///
/// # Implementing a new driver
///
/// ```rust,ignore
/// use dbench_core::{Driver, Connection, ConnectionInfo, Result};
///
/// pub struct MyDbDriver;
///
/// impl Driver for MyDbDriver {
///     type Connection = MyDbConnection;
///     type Config = MyDbConfig;
///
///     fn name(&self) -> &'static str { "mydb" }
///     fn database_type(&self) -> dbench_core::DatabaseType { todo!() }
///
///     async fn connect(&self, config: &Self::Config) -> Result<Self::Connection> {
///         todo!()
///     }
/// }
/// ```
pub trait Driver: Send + Sync + 'static {
    /// The type of connection this driver produces.
    type Connection: Connection;
    /// The configuration type required to open a connection.
    type Config: ConnectionConfig + Send + Sync;

    /// Short identifier for this driver (e.g., `"postgres"`, `"mongodb"`).
    fn name(&self) -> &'static str;

    /// The [`DatabaseType`](crate::DatabaseType) this driver connects to.
    fn database_type(&self) -> crate::DatabaseType;

    /// The default port for this database type, if applicable.
    fn default_port(&self) -> Option<u16> {
        None
    }

    /// Open a new connection using the given configuration.
    ///
    /// This should establish the TCP/socket connection, perform authentication,
    /// and return a ready-to-use [`Connection`].
    ///
    /// # Errors
    /// Returns [`CatalystError`] if the connection cannot be established.
    fn connect(
        &self,
        config: &Self::Config,
    ) -> impl std::future::Future<Output = Result<Self::Connection>> + Send;

    /// Test a connection without retaining it.
    ///
    /// Connects, pings, and immediately closes. Returns connection metadata.
    ///
    /// # Errors
    /// Returns [`CatalystError`] if the connection test fails.
    fn test_connection(
        &self,
        config: &Self::Config,
    ) -> impl std::future::Future<Output = Result<ConnectionInfo>> + Send {
        async move {
            let mut conn = self.connect(config).await?;
            let info = conn.info().clone();
            let _ = conn.ping().await;
            conn.close().await?;
            Ok(info)
        }
    }
}

/// Trait for driver configuration types.
///
/// Every driver configuration (e.g., `PostgresConfig`, `MongoConfig`) must
/// implement this trait. The `#[derive(ConnectionConfig)]` macro from
/// `dbench-macros` generates the implementation automatically.
pub trait ConnectionConfig: serde::Serialize + for<'de> serde::Deserialize<'de> {
    /// Validate the configuration values.
    ///
    /// Called before attempting to connect. Should verify required fields,
    /// port ranges, URL formats, etc.
    ///
    /// # Errors
    /// Returns [`CatalystError::Config`] if validation fails.
    fn validate(&self) -> Result<()>;

    /// Return a sanitized (no secrets) representation for display/logging.
    fn display_string(&self) -> String;
}
