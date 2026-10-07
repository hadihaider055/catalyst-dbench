//! Live smoke tests against real servers. Each test is skipped unless its env var is set.
//!
//! ```bash
//! docker run -d -p 1433:1433 -e ACCEPT_EULA=Y -e MSSQL_SA_PASSWORD='Dbench!2024' mcr.microsoft.com/mssql/server:2022-latest
//! docker run -d -p 8000:8000 surrealdb/surrealdb:latest start --user root --pass root
//! docker run -d -p 9200:9200 -e discovery.type=single-node -e DISABLE_SECURITY_PLUGIN=true opensearchproject/opensearch:2
//! docker run -d -p 8001:8000 amazon/dynamodb-local
//!
//! DBENCH_LIVE_MSSQL='Dbench!2024' DBENCH_LIVE_SURREAL=1 DBENCH_LIVE_ES=1 DBENCH_LIVE_DYNAMO=1 \
//!   cargo test -p dbench-drivers --features all --test live -- --nocapture
//! ```
#![cfg(feature = "all")]

use dbench_core::{Connection, ConnectionMode, Driver, Query};
use dbench_drivers::*;
use dbench_security::tls::{TlsConfig, TlsMode};

fn no_tls() -> TlsConfig {
    TlsConfig {
        mode: TlsMode::Disabled,
        ..Default::default()
    }
}

/// Run a read, then confirm a read-only connection refuses a write.
async fn check<D: Driver>(
    driver: D,
    rw: D::Config,
    ro: D::Config,
    read: Option<&str>,
    write: &str,
) {
    let mut conn = driver.connect(&rw).await.expect("connect");
    if let Some(read) = read {
        let r = conn.execute(&Query::new(read)).await.expect("read query");
        println!(
            "{read} -> {} rows, cols {:?}",
            r.rows.len(),
            r.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }
    conn.inspect_schema().await.expect("schema");
    conn.ping().await.expect("ping");

    let mut ro_conn = driver.connect(&ro).await.expect("connect read-only");
    let err = ro_conn
        .execute(&Query::new(write))
        .await
        .expect_err("write must be rejected");
    assert!(
        matches!(err, dbench_core::CatalystError::ReadOnlyViolation),
        "{err}"
    );
}

#[tokio::test]
async fn mssql() {
    let Ok(pw) = std::env::var("DBENCH_LIVE_MSSQL") else {
        return;
    };
    let cfg = MssqlConfig {
        password: Some(pw),
        tls: no_tls(),
        ..MssqlConfig::default()
    };
    let ro = MssqlConfig {
        mode: ConnectionMode::ReadOnly,
        ..cfg.clone()
    };
    check(
        MssqlDriver,
        cfg,
        ro,
        Some("SELECT name, database_id FROM sys.databases"),
        "/* hi */ DROP TABLE t",
    )
    .await;
}

#[tokio::test]
async fn surrealdb() {
    if std::env::var("DBENCH_LIVE_SURREAL").is_err() {
        return;
    }
    let cfg = SurrealConfig {
        password: Some("root".into()),
        tls: no_tls(),
        ..SurrealConfig::default()
    };
    let ro = SurrealConfig {
        mode: ConnectionMode::ReadOnly,
        ..cfg.clone()
    };
    let mut setup = SurrealDriver.connect(&cfg).await.expect("connect");
    setup
        .execute(&Query::new("CREATE person:one SET name = 'Ada';"))
        .await
        .expect("seed");
    check(
        SurrealDriver,
        cfg,
        ro,
        Some("SELECT * FROM person;"),
        "DELETE person;",
    )
    .await;
}

#[tokio::test]
async fn opensearch() {
    if std::env::var("DBENCH_LIVE_ES").is_err() {
        return;
    }
    let cfg = ElasticsearchConfig {
        tls: no_tls(),
        ..ElasticsearchConfig::default()
    };
    let ro = ElasticsearchConfig {
        mode: ConnectionMode::ReadOnly,
        ..cfg.clone()
    };
    let mut setup = ElasticsearchDriver.connect(&cfg).await.expect("connect");
    setup
        .execute(&Query::new(
            "PUT /dbench-test/_doc/1?refresh=true\n{\"name\": \"Ada\", \"age\": 36}",
        ))
        .await
        .expect("seed");
    check(
        ElasticsearchDriver,
        cfg,
        ro,
        Some("GET /dbench-test/_search"),
        "DELETE /dbench-test",
    )
    .await;
}

#[tokio::test]
async fn dynamodb_local() {
    if std::env::var("DBENCH_LIVE_DYNAMO").is_err() {
        return;
    }
    let cfg = DynamoConfig {
        endpoint: Some("http://localhost:8001".into()),
        access_key_id: "local".into(),
        secret_access_key: Some("local".into()),
        ..DynamoConfig::default()
    };
    let ro = DynamoConfig {
        mode: ConnectionMode::ReadOnly,
        ..cfg.clone()
    };
    // DynamoDB Local starts empty (PartiQL can't create tables): connect, schema, read-only guard.
    check(
        DynamoDriver,
        cfg,
        ro,
        None,
        "DELETE FROM \"t\" WHERE pk = 'x'",
    )
    .await;
}
