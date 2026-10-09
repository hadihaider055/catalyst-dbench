//! Type and feature matrix against real servers: every common column type must read
//! back with its value (not NULL), plus schema, explain, batch, read-only and errors.
//! Skipped unless `DBENCH_MATRIX=1`. Servers: see the `docker run` lines in live.rs.
//!
//! ```bash
//! DBENCH_MATRIX=1 cargo test -p dbench-drivers --features all --test matrix -- --nocapture --test-threads=1
//! ```
#![cfg(feature = "all")]

use dbench_core::{CatalystError, Connection, ConnectionMode, Driver, Query, QueryResult, Value};
use dbench_drivers::*;
use dbench_security::tls::{TlsConfig, TlsMode};

fn enabled() -> bool {
    std::env::var("DBENCH_MATRIX").is_ok()
}

fn no_tls() -> TlsConfig {
    TlsConfig {
        mode: TlsMode::Disabled,
        ..Default::default()
    }
}

fn ro<C: Clone>(cfg: &C, set: impl FnOnce(&mut C)) -> C {
    let mut c = cfg.clone();
    set(&mut c);
    c
}

struct Report {
    name: &'static str,
    ok: usize,
    fails: Vec<String>,
}

impl Report {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            ok: 0,
            fails: vec![],
        }
    }
    fn check(&mut self, cond: bool, msg: impl Into<String>) {
        if cond {
            self.ok += 1;
        } else {
            self.fails.push(msg.into());
        }
    }
    fn fail(&mut self, msg: impl Into<String>) {
        self.check(false, msg);
    }
    fn finish(self) {
        println!(
            "\n== {}: {} passed, {} failed",
            self.name,
            self.ok,
            self.fails.len()
        );
        for f in &self.fails {
            println!("   ✗ {f}");
        }
        assert!(self.fails.is_empty(), "{} had failures", self.name);
    }
}

/// `want` is a `|`-separated list of accepted substrings (case-insensitive), or "NULL".
fn matches(v: &Value, want: &str) -> bool {
    if want == "NULL" {
        return v.is_null();
    }
    if v.is_null() {
        return false;
    }
    let d = v.display().to_lowercase();
    want.split('|').any(|w| d.contains(&w.to_lowercase()))
}

fn check_row(r: &mut Report, res: &QueryResult, row: usize, expect: &[(&str, &str)]) {
    let Some(values) = res.rows.get(row) else {
        r.fail(format!(
            "row {row} missing ({} rows returned)",
            res.rows.len()
        ));
        return;
    };
    for (col, want) in expect {
        match res
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(col))
        {
            None => r.fail(format!(
                "column `{col}` missing; got {:?}",
                res.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
            )),
            Some(i) => {
                let v = &values.values[i];
                r.check(
                    matches(v, want),
                    format!(
                        "`{col}` ({}): want {want:?}, got {:?}",
                        res.columns[i].native_type, v
                    ),
                );
            }
        }
    }
}

struct Case<'a> {
    setup: &'a [&'a str],
    /// Row 0 holds values, row 1 (if `null_cols` is non-empty) holds NULLs.
    select: &'a str,
    expect: &'a [(&'a str, &'a str)],
    null_cols: &'a [&'a str],
    /// Text that must appear in the serialized schema.
    schema_has: &'a str,
    explain: bool,
    batch: [&'a str; 2],
    write: &'a str,
    bad: &'a str,
}

async fn run<D: Driver>(
    name: &'static str,
    driver: D,
    rw: D::Config,
    ro: D::Config,
    case: Case<'_>,
) {
    let mut r = Report::new(name);
    let mut c = match driver.connect(&rw).await {
        Ok(c) => c,
        Err(e) => {
            r.fail(format!("connect: {e}"));
            return r.finish();
        }
    };
    for s in case.setup {
        if let Err(e) = c.execute(&Query::new(*s)).await {
            r.fail(format!("setup `{}`: {e}", s.lines().next().unwrap_or(s)));
        }
    }

    match c.execute(&Query::new(case.select)).await {
        Ok(res) => {
            check_row(&mut r, &res, 0, case.expect);
            if !case.null_cols.is_empty() {
                let nulls: Vec<_> = case.null_cols.iter().map(|c| (*c, "NULL")).collect();
                check_row(&mut r, &res, 1, &nulls);
            }
        }
        Err(e) => r.fail(format!("select: {e}")),
    }

    match c.inspect_schema().await {
        Ok(s) => {
            let j = serde_json::to_string(&s).unwrap_or_default();
            r.check(
                j.contains(case.schema_has),
                format!("schema lists `{}`", case.schema_has),
            );
        }
        Err(e) => r.fail(format!("schema: {e}")),
    }

    if case.explain {
        match c.execute(&Query::new(case.select).explain()).await {
            Ok(res) => r.check(
                res.explain_plan
                    .as_deref()
                    .is_some_and(|p| !p.trim().is_empty())
                    || !res.rows.is_empty(),
                "explain returns a plan",
            ),
            Err(e) => r.fail(format!("explain: {e}")),
        }
    }

    match c
        .execute_batch(&[Query::new(case.batch[0]), Query::new(case.batch[1])])
        .await
    {
        Ok(res) => r.check(
            res.len() == 2,
            format!("batch returned {} results", res.len()),
        ),
        Err(e) => r.fail(format!("batch: {e}")),
    }

    if let Err(e) = c.ping().await {
        r.fail(format!("ping: {e}"));
    }

    match c.execute(&Query::new(case.bad)).await {
        Ok(_) => r.fail(format!("invalid query `{}` succeeded", case.bad)),
        Err(e) => {
            println!("   (error for bad query: {e})");
            r.check(
                e.to_string().len() > 15,
                format!("bad-query error too vague: {e}"),
            );
        }
    }

    match driver.connect(&ro).await {
        Ok(mut ro) => {
            if let Err(e) = ro.execute(&Query::new(case.select)).await {
                r.fail(format!("read-only select: {e}"));
            }
            match ro.execute(&Query::new(case.write)).await {
                Err(CatalystError::ReadOnlyViolation) => r.check(true, ""),
                Err(e) => r.fail(format!("read-only write: wrong error {e}")),
                Ok(_) => r.fail(format!("read-only write `{}` was allowed", case.write)),
            }
        }
        Err(e) => r.fail(format!("read-only connect: {e}")),
    }
    r.finish();
}

#[tokio::test]
async fn postgres() {
    if !enabled() {
        return;
    }
    let cfg = PostgresConfig {
        host: "localhost".into(),
        port: 5432,
        database: "postgres".into(),
        username: std::env::var("USER").unwrap_or_else(|_| "postgres".into()),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
        application_name: None,
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("postgres", PostgresDriver, cfg, ro, Case {
        setup: &[
            "DROP TABLE IF EXISTS dbench_matrix",
            "DROP TYPE IF EXISTS dbench_mood",
            "CREATE TYPE dbench_mood AS ENUM ('happy', 'sad')",
            "CREATE TABLE dbench_matrix (id int PRIMARY KEY, c_smallint smallint, c_int int, c_bigint bigint,
               c_real real, c_double double precision, c_numeric numeric(12,3), c_money money, c_text text,
               c_varchar varchar(10), c_char char(3), c_bool boolean, c_date date, c_time time, c_timetz timetz,
               c_timestamp timestamp, c_timestamptz timestamptz, c_interval interval, c_json json, c_jsonb jsonb,
               c_uuid uuid, c_bytea bytea, c_int_array int[], c_text_array text[], c_inet inet, c_cidr cidr,
               c_macaddr macaddr, c_enum dbench_mood, c_xml xml, c_point point, c_bit bit(4), c_oid oid,
               c_tsvector tsvector)",
            "INSERT INTO dbench_matrix VALUES (1, 7, 42, 9007199254740993, 1.5, 2.25, -1234.567, 12.34, 'héllo',
               'v', 'ab', true, '2024-02-29', '13:45:01', '13:45:01+02', '2024-01-02 03:04:05',
               '2024-01-02 03:04:05+00', '1 day 02:00:00', '{\"a\":1}', '{\"b\":2}',
               'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', '\\xdeadbeef', '{1,2}', '{x,y}', '10.0.0.1', '10.0.0.0/8',
               '08:00:2b:01:02:03', 'happy', '<a>1</a>', '(1,2)', B'1010', 42, 'fat cat')",
            "INSERT INTO dbench_matrix (id) VALUES (2)",
        ],
        select: "SELECT * FROM dbench_matrix ORDER BY id",
        expect: &[
            ("c_smallint", "7"), ("c_int", "42"), ("c_bigint", "9007199254740993"), ("c_real", "1.5"),
            ("c_double", "2.25"), ("c_numeric", "-1234.567"), ("c_money", "12.34"), ("c_text", "héllo"),
            ("c_varchar", "v"), ("c_char", "ab"), ("c_bool", "true"), ("c_date", "2024-02-29"),
            ("c_time", "13:45:01"), ("c_timetz", "13:45:01"), ("c_timestamp", "2024-01-02"),
            ("c_timestamptz", "2024-01-02"), ("c_interval", "1 day|26:00|26h"), ("c_json", "\"a\":1"),
            ("c_jsonb", "\"b\":2"), ("c_uuid", "a0eebc99"), ("c_bytea", "deadbeef"), ("c_int_array", "1"),
            ("c_text_array", "x"), ("c_inet", "10.0.0.1"), ("c_cidr", "10.0.0.0"), ("c_macaddr", "08:00:2b"),
            ("c_enum", "happy"), ("c_xml", "<a>1</a>"), ("c_point", "1"), ("c_bit", "1010"), ("c_oid", "42"),
            ("c_tsvector", "cat"),
        ],
        null_cols: &["c_int", "c_numeric", "c_text", "c_bool", "c_timestamptz", "c_jsonb", "c_uuid", "c_bytea"],
        schema_has: "dbench_matrix",
        explain: true,
        batch: ["SELECT 1", "SELECT 2"],
        write: "/* sneaky */ DELETE FROM dbench_matrix",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn cockroachdb() {
    if !enabled() {
        return;
    }
    let cfg = PostgresConfig {
        host: "localhost".into(),
        port: 26257,
        database: "defaultdb".into(),
        username: "root".into(),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
        application_name: None,
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("cockroachdb", PostgresDriver, cfg, ro, Case {
        setup: &[
            "DROP TABLE IF EXISTS dbench_matrix",
            "CREATE TABLE dbench_matrix (id int PRIMARY KEY, c_int2 int2, c_int8 int8, c_float8 float8,
               c_decimal decimal(12,3), c_text text, c_bool bool, c_date date, c_time time,
               c_timestamptz timestamptz, c_interval interval, c_jsonb jsonb, c_uuid uuid, c_bytes bytes,
               c_int_array int[], c_inet inet)",
            "INSERT INTO dbench_matrix VALUES (1, 7, 9007199254740993, 2.25, -1234.567, 'héllo', true,
               '2024-02-29', '13:45:01', '2024-01-02 03:04:05+00', '1 day', '{\"b\":2}',
               'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', b'\\xde\\xad', ARRAY[1,2], '10.0.0.1')",
            "INSERT INTO dbench_matrix (id) VALUES (2)",
        ],
        select: "SELECT * FROM dbench_matrix ORDER BY id",
        expect: &[
            ("c_int2", "7"), ("c_int8", "9007199254740993"), ("c_float8", "2.25"), ("c_decimal", "-1234.567"),
            ("c_text", "héllo"), ("c_bool", "true"), ("c_date", "2024-02-29"), ("c_time", "13:45:01"),
            ("c_timestamptz", "2024-01-02"), ("c_interval", "1 day|24:00"), ("c_jsonb", "\"b\":2"),
            ("c_uuid", "a0eebc99"), ("c_bytes", "dead"), ("c_int_array", "1"), ("c_inet", "10.0.0.1"),
        ],
        null_cols: &["c_int8", "c_decimal", "c_text", "c_jsonb"],
        schema_has: "dbench_matrix",
        explain: true,
        batch: ["SELECT 1", "SELECT 2"],
        write: "DELETE FROM dbench_matrix",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn mysql() {
    if !enabled() {
        return;
    }
    let cfg = MysqlConfig {
        host: "127.0.0.1".into(),
        port: 3306,
        database: "mysql".into(),
        username: "root".into(),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
    };
    let mut setup = MysqlDriver.connect(&cfg).await.expect("connect");
    setup
        .execute(&Query::new("CREATE DATABASE IF NOT EXISTS dbench_matrix"))
        .await
        .expect("create db");
    let cfg = MysqlConfig {
        database: "dbench_matrix".into(),
        ..cfg
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("mysql", MysqlDriver, cfg, ro, Case {
        setup: &[
            "DROP TABLE IF EXISTS dbench_matrix",
            "CREATE TABLE dbench_matrix (id int PRIMARY KEY, c_tinyint tinyint, c_smallint smallint,
               c_mediumint mediumint, c_int int, c_bigint bigint, c_ubigint bigint unsigned, c_float float,
               c_double double, c_decimal decimal(12,3), c_char char(3), c_varchar varchar(10), c_text text,
               c_blob blob, c_varbinary varbinary(8), c_date date, c_time time, c_datetime datetime,
               c_timestamp timestamp NULL, c_year year, c_json json, c_enum enum('a','b'), c_set set('x','y'),
               c_bit bit(8), c_bool bool)",
            "INSERT INTO dbench_matrix VALUES (1, -5, 7, 300000, 42, 9007199254740993, 18446744073709551615,
               1.5, 2.25, -1234.567, 'ab', 'v', 'héllo', x'DEADBEEF', x'CAFE', '2024-02-29', '13:45:01',
               '2024-01-02 03:04:05', '2024-01-02 03:04:05', 2024, '{\"a\": 1}', 'b', 'x,y', b'1010', true)",
            "INSERT INTO dbench_matrix (id) VALUES (2)",
        ],
        select: "SELECT * FROM dbench_matrix ORDER BY id",
        expect: &[
            ("c_tinyint", "-5"), ("c_smallint", "7"), ("c_mediumint", "300000"), ("c_int", "42"),
            ("c_bigint", "9007199254740993"), ("c_ubigint", "18446744073709551615"), ("c_float", "1.5"),
            ("c_double", "2.25"), ("c_decimal", "-1234.567"), ("c_char", "ab"), ("c_varchar", "v"),
            ("c_text", "héllo"), ("c_blob", "deadbeef"), ("c_varbinary", "cafe"), ("c_date", "2024-02-29"),
            ("c_time", "13:45:01"), ("c_datetime", "2024-01-02"), ("c_timestamp", "2024-01-02"),
            ("c_year", "2024"), ("c_json", "\"a\""), ("c_enum", "b"), ("c_set", "x,y"),
            ("c_bit", "10|1010|0a"), ("c_bool", "1|true"),
        ],
        null_cols: &["c_int", "c_decimal", "c_text", "c_blob", "c_datetime", "c_json"],
        schema_has: "dbench_matrix",
        explain: true,
        batch: ["SELECT 1", "SELECT 2"],
        write: "/* sneaky */ DELETE FROM dbench_matrix",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn sqlite() {
    if !enabled() {
        return;
    }
    let path = std::env::temp_dir().join("dbench_matrix.db");
    let _ = std::fs::remove_file(&path);
    let cfg = SqliteConfig {
        path: path.to_string_lossy().into_owned(),
        mode: ConnectionMode::ReadWrite,
        wal_mode: false,
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("sqlite", SqliteDriver, cfg, ro, Case {
        setup: &[
            "CREATE TABLE dbench_matrix (id INTEGER PRIMARY KEY, c_int INTEGER, c_big INTEGER, c_real REAL,
               c_text TEXT, c_blob BLOB, c_numeric NUMERIC, c_decimal DECIMAL(10,2), c_bool BOOLEAN,
               c_date DATE, c_datetime DATETIME, c_json JSON)",
            "INSERT INTO dbench_matrix VALUES (1, 42, 9007199254740993, 2.25, 'héllo', x'DEADBEEF', 1234.5,
               '19.99', 1, '2024-02-29', '2024-01-02 03:04:05', '{\"a\":1}')",
            "INSERT INTO dbench_matrix (id) VALUES (2)",
        ],
        select: "SELECT * FROM dbench_matrix ORDER BY id",
        expect: &[
            ("c_int", "42"), ("c_big", "9007199254740993"), ("c_real", "2.25"), ("c_text", "héllo"),
            ("c_blob", "deadbeef"), ("c_numeric", "1234.5"), ("c_decimal", "19.99"), ("c_bool", "1|true"),
            ("c_date", "2024-02-29"), ("c_datetime", "2024-01-02"), ("c_json", "\"a\""),
        ],
        null_cols: &["c_int", "c_real", "c_text", "c_blob"],
        schema_has: "dbench_matrix",
        explain: true,
        batch: ["SELECT 1", "SELECT 2"],
        write: "/* sneaky */ DELETE FROM dbench_matrix",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn mssql() {
    if !enabled() {
        return;
    }
    let cfg = MssqlConfig {
        password: Some("Dbench!2024".into()),
        tls: no_tls(),
        ..MssqlConfig::default()
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("sql server", MssqlDriver, cfg, ro, Case {
        setup: &[
            "DROP TABLE IF EXISTS dbench_matrix",
            "CREATE TABLE dbench_matrix (id int PRIMARY KEY, c_tinyint tinyint, c_smallint smallint, c_int int,
               c_bigint bigint, c_real real, c_float float, c_decimal decimal(12,3), c_numeric numeric(10,2),
               c_money money, c_smallmoney smallmoney, c_bit bit, c_char char(3), c_varchar varchar(10),
               c_nvarchar nvarchar(10), c_text text, c_ntext ntext, c_date date, c_time time,
               c_datetime datetime, c_datetime2 datetime2, c_smalldatetime smalldatetime,
               c_datetimeoffset datetimeoffset, c_uuid uniqueidentifier, c_varbinary varbinary(8), c_xml xml)",
            "INSERT INTO dbench_matrix VALUES (1, 5, 7, 42, 9007199254740993, 1.5, 2.25, -1234.567, 19.99,
               12.34, 5.5, 1, 'ab', 'v', N'héllo', 'long text', N'ntext', '2024-02-29', '13:45:01',
               '2024-01-02 03:04:05', '2024-01-02 03:04:05.1234567', '2024-01-02 03:04:00',
               '2024-01-02 03:04:05 +02:00', 'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11', 0xDEADBEEF, '<a>1</a>')",
            "INSERT INTO dbench_matrix (id) VALUES (2)",
        ],
        select: "SELECT * FROM dbench_matrix ORDER BY id",
        expect: &[
            ("c_tinyint", "5"), ("c_smallint", "7"), ("c_int", "42"), ("c_bigint", "9007199254740993"),
            ("c_real", "1.5"), ("c_float", "2.25"), ("c_decimal", "-1234.567"), ("c_numeric", "19.99"),
            ("c_money", "12.34"), ("c_smallmoney", "5.5"), ("c_bit", "true|1"), ("c_char", "ab"),
            ("c_varchar", "v"), ("c_nvarchar", "héllo"), ("c_text", "long text"), ("c_ntext", "ntext"),
            ("c_date", "2024-02-29"), ("c_time", "13:45:01"), ("c_datetime", "2024-01-02"),
            ("c_datetime2", "2024-01-02"), ("c_smalldatetime", "2024-01-02"), ("c_datetimeoffset", "2024-01-02T01:04:05"),
            ("c_uuid", "a0eebc99"), ("c_varbinary", "deadbeef"), ("c_xml", "<a>1</a>"),
        ],
        null_cols: &["c_int", "c_decimal", "c_nvarchar", "c_datetime2", "c_uuid"],
        schema_has: "dbench_matrix",
        explain: false,
        batch: ["SELECT 1 AS a", "SELECT 2 AS b"],
        write: "/* sneaky */ DELETE FROM dbench_matrix",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn clickhouse() {
    if !enabled() {
        return;
    }
    let cfg = ClickhouseConfig {
        host: "localhost".into(),
        port: 8123,
        database: "default".into(),
        username: "default".into(),
        password: Some("dbench".into()),
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("clickhouse", ClickhouseDriver, cfg, ro, Case {
        setup: &[
            "DROP TABLE IF EXISTS dbench_matrix",
            "CREATE TABLE dbench_matrix (id UInt32, c_int8 Int8, c_int64 Int64, c_uint64 UInt64,
               c_float32 Float32, c_float64 Float64, c_dec Decimal(12,3), c_str String, c_fstr FixedString(3),
               c_date Date, c_date32 Date32, c_dt DateTime, c_dt64 DateTime64(3), c_uuid UUID, c_bool Bool,
               c_arr Array(Int32), c_map Map(String, Int32), c_ipv4 IPv4, c_enum Enum8('a' = 1, 'b' = 2),
               c_lc LowCardinality(String), c_nullable Nullable(Int32), c_tuple Tuple(Int32, String))
               ENGINE = MergeTree ORDER BY id",
            "INSERT INTO dbench_matrix VALUES (1, -5, 9007199254740993, 18446744073709551615, 1.5, 2.25,
               -1234.567, 'héllo', 'abc', '2024-02-29', '2024-02-29', '2024-01-02 03:04:05',
               '2024-01-02 03:04:05.123', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', true, [1, 2], {'k': 1},
               '10.0.0.1', 'b', 'low', NULL, (1, 'x'))",
        ],
        select: "SELECT * FROM dbench_matrix ORDER BY id",
        expect: &[
            ("c_int8", "-5"), ("c_int64", "9007199254740993"), ("c_uint64", "18446744073709551615"),
            ("c_float32", "1.5"), ("c_float64", "2.25"), ("c_dec", "-1234.567"), ("c_str", "héllo"),
            ("c_fstr", "abc"), ("c_date", "2024-02-29"), ("c_date32", "2024-02-29"), ("c_dt", "2024-01-02"),
            ("c_dt64", "2024-01-02"), ("c_uuid", "a0eebc99"), ("c_bool", "true"), ("c_arr", "1"),
            ("c_map", "k"), ("c_ipv4", "10.0.0.1"), ("c_enum", "b"), ("c_lc", "low"), ("c_nullable", "NULL"),
            ("c_tuple", "x"),
        ],
        null_cols: &[],
        schema_has: "dbench_matrix",
        explain: true,
        batch: ["SELECT 1", "SELECT 2"],
        write: "/* sneaky */ INSERT INTO dbench_matrix (id) VALUES (9)",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn cassandra() {
    if !enabled() {
        return;
    }
    let base = CassandraConfig {
        host: "localhost".into(),
        port: 9042,
        database: String::new(),
        username: String::new(),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(10000),
    };
    let mut setup = CassandraDriver.connect(&base).await.expect("connect");
    setup
        .execute(&Query::new(
            "CREATE KEYSPACE IF NOT EXISTS dbench WITH replication = {'class': 'SimpleStrategy', 'replication_factor': 1}",
        ))
        .await
        .expect("keyspace");
    let cfg = CassandraConfig {
        database: "dbench".into(),
        ..base
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("cassandra", CassandraDriver, cfg, ro, Case {
        setup: &[
            "DROP TABLE IF EXISTS dbench_matrix",
            "CREATE TABLE dbench_matrix (id int PRIMARY KEY, c_tinyint tinyint, c_smallint smallint, c_int int,
               c_bigint bigint, c_varint varint, c_decimal decimal, c_float float, c_double double, c_text text,
               c_ascii ascii, c_bool boolean, c_date date, c_time time, c_ts timestamp, c_uuid uuid,
               c_timeuuid timeuuid, c_blob blob, c_inet inet, c_list list<int>, c_set set<text>,
               c_map map<text, int>, c_duration duration)",
            "INSERT INTO dbench_matrix (id, c_tinyint, c_smallint, c_int, c_bigint, c_varint, c_decimal, c_float,
               c_double, c_text, c_ascii, c_bool, c_date, c_time, c_ts, c_uuid, c_timeuuid, c_blob, c_inet,
               c_list, c_set, c_map, c_duration) VALUES (1, 5, 7, 42, 9007199254740993,
               123456789012345678901234567890, -1234.567, 1.5, 2.25, 'héllo', 'abc', true, '2024-02-29',
               '13:45:01', '2024-01-02 03:04:05+0000', a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11, now(), 0xdeadbeef,
               '10.0.0.1', [1, 2], {'x', 'y'}, {'k': 1}, 1h2m)",
            "INSERT INTO dbench_matrix (id) VALUES (2)",
        ],
        select: "SELECT * FROM dbench_matrix WHERE id IN (1, 2)",
        expect: &[
            ("c_tinyint", "5"), ("c_smallint", "7"), ("c_int", "42"), ("c_bigint", "9007199254740993"),
            ("c_varint", "123456789012345678901234567890"), ("c_decimal", "-1234.567"), ("c_float", "1.5"),
            ("c_double", "2.25"), ("c_text", "héllo"), ("c_ascii", "abc"), ("c_bool", "true"),
            ("c_date", "2024-02-29"), ("c_time", "13:45:01"), ("c_ts", "2024-01-02"), ("c_uuid", "a0eebc99"),
            ("c_timeuuid", "-"), ("c_blob", "deadbeef"), ("c_inet", "10.0.0.1"), ("c_list", "1"),
            ("c_set", "x"), ("c_map", "k"), ("c_duration", "1h|62|3720"),
        ],
        null_cols: &["c_int", "c_decimal", "c_text", "c_list"],
        schema_has: "dbench_matrix",
        explain: false,
        batch: ["SELECT now() FROM system.local", "SELECT release_version FROM system.local"],
        write: "DELETE FROM dbench_matrix WHERE id = 1",
        bad: "SELEC 1",
    })
    .await;
}

#[tokio::test]
async fn mongodb() {
    if !enabled() {
        return;
    }
    let cfg = MongoConfig {
        uri: None,
        host: "localhost".into(),
        port: 27017,
        database: "dbench_matrix".into(),
        username: None,
        password: None,
        auth_source: None,
        replica_set: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("mongodb", MongoDriver, cfg, ro, Case {
        setup: &[
            r#"{"delete": "matrix", "deletes": [{"q": {}, "limit": 0}]}"#,
            r#"{"insert": "matrix", "documents": [
                {"_id": 1, "i32": {"$numberInt": "42"}, "i64": {"$numberLong": "9007199254740993"},
                 "dbl": 2.25, "dec": {"$numberDecimal": "-1234.567"}, "str": "héllo", "bool": true,
                 "date": {"$date": "2024-01-02T03:04:05Z"}, "oid": {"$oid": "65a1b2c3d4e5f60718293a4b"},
                 "bin": {"$binary": {"base64": "3q2+7w==", "subType": "00"}}, "arr": [1, 2],
                 "doc": {"a": 1}, "nul": null},
                {"_id": 2, "i32": null}]}"#,
        ],
        select: r#"{"find": "matrix", "filter": {}, "sort": {"_id": 1}}"#,
        expect: &[
            ("i32", "42"), ("i64", "9007199254740993"), ("dbl", "2.25"), ("dec", "-1234.567"),
            ("str", "héllo"), ("bool", "true"), ("date", "2024-01-02"), ("oid", "65a1b2c3"),
            ("bin", "deadbeef|3q2"), ("arr", "1"), ("doc", "\"a\""), ("nul", "NULL"),
        ],
        null_cols: &["i32"],
        schema_has: "matrix",
        explain: true,
        batch: [
            r#"{"aggregate": "matrix", "pipeline": [{"$count": "n"}]}"#,
            r#"{"find": "matrix", "filter": {"_id": 1}}"#,
        ],
        write: r#"{"delete": "matrix", "deletes": [{"q": {}, "limit": 0}]}"#,
        bad: r#"{"find": "matrix", "filter": "#,
    })
    .await;
}

#[tokio::test]
async fn redis() {
    if !enabled() {
        return;
    }
    let cfg = RedisConfig {
        host: "localhost".into(),
        port: 6379,
        db_index: 0,
        password: None,
        username: None,
        tls: false,
        mode: ConnectionMode::ReadWrite,
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    let mut r = Report::new("redis");
    let mut c = RedisDriver.connect(&cfg).await.expect("connect");
    let k = "dbench:matrix";
    for cmd in [
        format!("DEL {k}:s {k}:h {k}:l {k}:set {k}:z {k}:x {k}:n"),
        format!("SET {k}:s \"héllo world\""),
        format!("HSET {k}:h name Ada age 36"),
        format!("RPUSH {k}:l a b c"),
        format!("SADD {k}:set x y"),
        format!("ZADD {k}:z 1.5 one 2 two"),
        format!("XADD {k}:x * field value"),
        format!("SET {k}:n 41"),
        format!("INCR {k}:n"),
    ] {
        if let Err(e) = c.execute(&Query::new(&cmd)).await {
            r.fail(format!("`{cmd}`: {e}"));
        }
    }
    for (cmd, want) in [
        (format!("GET {k}:s"), "héllo world"),
        (format!("HGETALL {k}:h"), "ada"),
        (format!("LRANGE {k}:l 0 -1"), "b"),
        (format!("SMEMBERS {k}:set"), "x|y"),
        (format!("ZRANGE {k}:z 0 -1 WITHSCORES"), "1.5"),
        (format!("XRANGE {k}:x - +"), "value"),
        (format!("GET {k}:n"), "42"),
        (format!("TYPE {k}:h"), "hash"),
        (format!("TTL {k}:s"), "-1"),
        (format!("GET {k}:missing"), "NULL"),
    ] {
        match c.execute(&Query::new(&cmd)).await {
            Ok(res) => {
                let all: Vec<&Value> = res.rows.iter().flat_map(|r| r.values.iter()).collect();
                let ok = if want == "NULL" {
                    all.iter().all(|v| v.is_null())
                } else {
                    all.iter().any(|v| matches(v, want))
                };
                r.check(ok, format!("`{cmd}`: want {want:?}, got {:?}", all));
            }
            Err(e) => r.fail(format!("`{cmd}`: {e}")),
        }
    }
    match c.inspect_schema().await {
        Ok(s) => r.check(
            serde_json::to_string(&s)
                .unwrap_or_default()
                .contains("dbench"),
            "schema lists dbench:* keys",
        ),
        Err(e) => r.fail(format!("schema: {e}")),
    }
    match c.execute(&Query::new("NOTACOMMAND x")).await {
        Ok(_) => r.fail("unknown command succeeded"),
        Err(e) => println!("   (error for bad command: {e})"),
    }
    let mut roc = RedisDriver.connect(&ro).await.expect("ro connect");
    match roc.execute(&Query::new(format!("SET {k}:s nope"))).await {
        Err(CatalystError::ReadOnlyViolation) => r.check(true, ""),
        other => r.fail(format!("read-only SET: {other:?}")),
    }
    let _ = c
        .execute(&Query::new(format!(
            "DEL {k}:s {k}:h {k}:l {k}:set {k}:z {k}:x {k}:n"
        )))
        .await;
    r.finish();
}

#[tokio::test]
async fn opensearch() {
    if !enabled() {
        return;
    }
    let cfg = ElasticsearchConfig {
        tls: no_tls(),
        ..ElasticsearchConfig::default()
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("opensearch", ElasticsearchDriver, cfg, ro, Case {
        setup: &[
            "PUT /dbench-matrix/_doc/1?refresh=true\n{\"i\": 42, \"big\": 9007199254740993, \"f\": 2.25, \"s\": \"héllo\", \"b\": true, \"d\": \"2024-01-02T03:04:05Z\", \"arr\": [1, 2], \"obj\": {\"a\": 1}, \"nul\": null}",
        ],
        select: "GET /dbench-matrix/_search",
        expect: &[
            ("i", "42"), ("big", "9007199254740993"), ("f", "2.25"), ("s", "héllo"), ("b", "true"),
            ("d", "2024-01-02"), ("arr", "1"), ("obj", "a"), ("nul", "NULL"),
        ],
        null_cols: &[],
        schema_has: "dbench-matrix",
        explain: false,
        batch: ["GET /_cluster/health", "SELECT i, s FROM `dbench-matrix`"],
        write: "DELETE /dbench-matrix",
        bad: "GET /dbench-missing-index/_search",
    })
    .await;
}

#[tokio::test]
async fn surrealdb() {
    if !enabled() {
        return;
    }
    let cfg = SurrealConfig {
        password: Some("root".into()),
        tls: no_tls(),
        ..SurrealConfig::default()
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("surrealdb", SurrealDriver, cfg, ro, Case {
        setup: &[
            "REMOVE TABLE IF EXISTS matrix;",
            "CREATE matrix:one SET i = 42, big = 9007199254740993, dec = -1234.567dec, f = 2.25f, s = 'héllo',
               b = true, dt = d'2024-01-02T03:04:05Z', u = u'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', arr = [1, 2],
               obj = { a: 1 }, nul = NULL, dur = 1h2m;",
        ],
        select: "SELECT * FROM matrix:one;",
        expect: &[
            ("i", "42"), ("big", "9007199254740993"), ("dec", "-1234.567"), ("f", "2.25"), ("s", "héllo"),
            ("b", "true"), ("dt", "2024-01-02"), ("u", "a0eebc99"), ("arr", "1"), ("obj", "a"),
            ("nul", "NULL"), ("dur", "1h2m"), ("id", "matrix:one"),
        ],
        null_cols: &[],
        schema_has: "matrix",
        explain: false,
        batch: ["RETURN 1;", "INFO FOR DB;"],
        write: "DELETE matrix;",
        bad: "SELEC * FROM matrix;",
    })
    .await;
}

#[tokio::test]
async fn dynamodb() {
    if !enabled() {
        return;
    }
    // Table must exist (PartiQL can't create tables); the test runner creates "matrix" with pk (S).
    let cfg = DynamoConfig {
        endpoint: Some("http://localhost:8001".into()),
        access_key_id: "local".into(),
        secret_access_key: Some("local".into()),
        ..DynamoConfig::default()
    };
    let ro = ro(&cfg, |c| c.mode = ConnectionMode::ReadOnly);
    run("dynamodb", DynamoDriver, cfg, ro, Case {
        setup: &[
            "DELETE FROM \"matrix\" WHERE pk = '1'",
            "INSERT INTO \"matrix\" VALUE {'pk': '1', 'n': 42, 'big': 9007199254740993, 'd': -1234.567,
               's': 'héllo', 'b': true, 'l': [1, 'two'], 'm': {'a': 1}, 'nul': NULL, 'ss': <<'x', 'y'>>,
               'ns': <<1, 2>>}",
        ],
        select: "SELECT * FROM \"matrix\" WHERE pk = '1'",
        expect: &[
            ("n", "42"), ("big", "9007199254740993"), ("d", "-1234.567"), ("s", "héllo"), ("b", "true"),
            ("l", "two"), ("m", "a"), ("nul", "NULL"), ("ss", "x"), ("ns", "1"),
        ],
        null_cols: &[],
        schema_has: "matrix",
        explain: false,
        batch: ["SELECT * FROM \"matrix\"", "SELECT pk FROM \"matrix\""],
        write: "DELETE FROM \"matrix\" WHERE pk = '1'",
        bad: "SELEC * FROM \"matrix\"",
    })
    .await;
}

// ---------------------------------------------------------------------------
// Complex queries: joins, subqueries, CTEs, window functions, set operations.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Dialect {
    Postgres,
    Cockroach,
    Mysql,
    Sqlite,
    Mssql,
    Clickhouse,
}

struct CCase {
    what: &'static str,
    sql: String,
    rows: Option<usize>,
    cols: Option<usize>,
    /// (row, column, wanted) — same matching rules as `matches`.
    cells: Vec<(usize, &'static str, &'static str)>,
}

fn cc(
    what: &'static str,
    sql: impl Into<String>,
    rows: Option<usize>,
    cells: &[(usize, &'static str, &'static str)],
) -> CCase {
    CCase {
        what,
        sql: sql.into(),
        rows,
        cols: None,
        cells: cells.to_vec(),
    }
}

fn complex_setup(d: Dialect) -> Vec<String> {
    use Dialect::*;
    let (engine, nstr) = match d {
        Clickhouse => (" ENGINE = Memory", "Nullable(String)"),
        _ => ("", "VARCHAR(20)"),
    };
    let int = if d == Clickhouse { "Int32" } else { "INT" };
    let s = if d == Clickhouse {
        "String"
    } else {
        "VARCHAR(20)"
    };
    let date = if d == Clickhouse { "Date" } else { "DATE" };
    let pk = if d == Clickhouse { "" } else { " PRIMARY KEY" };
    vec![
        "DROP TABLE IF EXISTS dbench_orders".to_string(),
        "DROP TABLE IF EXISTS dbench_customers".to_string(),
        format!("CREATE TABLE dbench_customers (id {int}{pk}, name {s}, country {nstr}){engine}"),
        format!("CREATE TABLE dbench_orders (id {int}{pk}, customer_id {int}, amount {int}, placed {date}){engine}"),
        "INSERT INTO dbench_customers VALUES (1, 'Ada', 'UK'), (2, 'Bob', 'US'), (3, 'Cy', 'US'), (4, 'Dee', NULL)".to_string(),
        "INSERT INTO dbench_orders VALUES (1, 1, 100, '2024-01-01'), (2, 1, 50, '2024-01-05'), (3, 2, 70, '2024-02-01'), (4, 3, 30, '2024-02-02'), (5, 3, 30, '2024-03-01')".to_string(),
    ]
}

fn complex_cases(d: Dialect) -> Vec<CCase> {
    use Dialect::*;
    let top = |n: u32, sql: &str| -> String {
        if d == Mssql {
            sql.replacen("SELECT", &format!("SELECT TOP {n}"), 1)
        } else {
            format!("{sql} LIMIT {n}")
        }
    };
    let recursive = if matches!(d, Mssql) { "" } else { "RECURSIVE " };
    let series = match d {
        Postgres | Cockroach => "SELECT count(*) AS n FROM (SELECT g FROM generate_series(1, 10000) g) s".to_string(),
        Clickhouse => "SELECT number AS g FROM numbers(10000)".to_string(),
        Mssql => "WITH n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 10000) SELECT x AS g FROM n OPTION (MAXRECURSION 0)".to_string(),
        Mysql | Sqlite => "WITH d(x) AS (SELECT 0 UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) SELECT a.x + 10 * b.x + 100 * c.x + 1000 * e.x AS g FROM d a, d b, d c, d e".to_string(),
    };
    let series_rows = if matches!(d, Postgres | Cockroach) {
        1
    } else {
        10000
    };
    let mut v = vec![
        cc("inner join + group by + having",
           "SELECT c.name, SUM(o.amount) AS total FROM dbench_customers c JOIN dbench_orders o ON o.customer_id = c.id GROUP BY c.name HAVING SUM(o.amount) > 50 ORDER BY total DESC",
           Some(3), &[(0, "name", "Ada"), (0, "total", "150"), (2, "name", "Cy")]),
        cc("three-way join",
           "SELECT c.name, o.amount, o2.amount AS other FROM dbench_customers c JOIN dbench_orders o ON o.customer_id = c.id JOIN dbench_orders o2 ON o2.customer_id = c.id AND o2.id > o.id ORDER BY c.name",
           Some(2), &[(0, "name", "Ada"), (0, "other", "50")]),
        cc("self join",
           "SELECT a.name, b.name AS other FROM dbench_customers a JOIN dbench_customers b ON a.country = b.country AND a.id < b.id",
           Some(1), &[(0, "name", "Bob"), (0, "other", "Cy")]),
        cc("IN subquery",
           "SELECT name FROM dbench_customers WHERE id IN (SELECT customer_id FROM dbench_orders WHERE amount >= 70) ORDER BY name",
           Some(2), &[(0, "name", "Ada"), (1, "name", "Bob")]),
        cc("derived table",
           "SELECT MAX(t.total) AS best FROM (SELECT customer_id, SUM(amount) AS total FROM dbench_orders GROUP BY customer_id) t",
           Some(1), &[(0, "best", "150")]),
        cc("CTE",
           "WITH t AS (SELECT customer_id, SUM(amount) AS total FROM dbench_orders GROUP BY customer_id) SELECT c.name, t.total FROM t JOIN dbench_customers c ON c.id = t.customer_id ORDER BY t.total DESC",
           Some(3), &[(0, "name", "Ada"), (0, "total", "150")]),
        cc("recursive CTE",
           format!("WITH {recursive}n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 5) SELECT SUM(x) AS s FROM n"),
           Some(1), &[(0, "s", "15")]),
        cc("window functions",
           "SELECT id, ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY amount DESC) AS rn, SUM(amount) OVER (PARTITION BY customer_id) AS cust_total FROM dbench_orders ORDER BY id",
           Some(5), &[(0, "rn", "1"), (0, "cust_total", "150"), (1, "rn", "2")]),
        cc("LAG",
           "SELECT id, amount - LAG(amount) OVER (ORDER BY id) AS diff FROM dbench_orders ORDER BY id",
           Some(5), &[(0, "diff", "NULL"), (1, "diff", "-50")]),
        cc("UNION",
           "SELECT country FROM dbench_customers WHERE country IS NOT NULL UNION SELECT 'FR' AS country ORDER BY country",
           Some(3), &[(0, "country", "FR")]),
        cc("UNION ALL",
           "SELECT name FROM dbench_customers UNION ALL SELECT name FROM dbench_customers",
           Some(8), &[]),
        cc("INTERSECT",
           "SELECT * FROM (SELECT customer_id FROM dbench_orders INTERSECT SELECT id FROM dbench_customers WHERE country = 'US') t ORDER BY customer_id",
           Some(2), &[(0, "customer_id", "2")]),
        cc("EXCEPT",
           "SELECT id FROM dbench_customers EXCEPT SELECT customer_id FROM dbench_orders",
           Some(1), &[(0, "id", "4")]),
        cc("CASE + COALESCE",
           "SELECT name, COALESCE(country, 'n/a') AS c, CASE WHEN id % 2 = 0 THEN 'even' ELSE 'odd' END AS parity FROM dbench_customers ORDER BY id",
           Some(4), &[(0, "parity", "odd"), (3, "c", "n/a"), (3, "parity", "even")]),
        cc("duplicate column names",
           "SELECT * FROM dbench_customers c JOIN dbench_orders o ON o.customer_id = c.id WHERE o.id = 1",
           Some(1), &[]),
        cc("empty result keeps columns",
           "SELECT id, name, country FROM dbench_customers WHERE 1 = 0",
           Some(0), &[]),
        cc("unicode and quotes",
           // T-SQL needs N'…' for non-ASCII literals.
           format!("SELECT {}'it''s ✓ 日本' AS s", if d == Mssql { "N" } else { "" }),
           Some(1), &[(0, "s", "it's ✓ 日本")]),
        cc("large result", series, Some(series_rows), &[]),
        cc("order + limit",
           top(2, "SELECT name FROM dbench_customers ORDER BY name DESC"),
           Some(2), &[(0, "name", "Dee")]),
    ];
    v[14].cols = Some(7);
    v[15].cols = Some(3);
    if d == Postgres || d == Cockroach {
        v[17].cells = vec![(0, "n", "10000")];
    }
    if d == Clickhouse {
        // ClickHouse dialect: lagInFrame, UNION DISTINCT, no recursive CTEs, and ON may
        // only hold equi-join conditions. Replace those cases with the native forms.
        let skip = [
            "three-way join",
            "self join",
            "recursive CTE",
            "LAG",
            "UNION",
            "INTERSECT",
        ];
        v.retain(|c| !skip.contains(&c.what));
        v.push(cc("three-way join (filter in WHERE)",
           "SELECT c.name, o.amount, o2.amount AS other FROM dbench_customers c JOIN dbench_orders o ON o.customer_id = c.id JOIN dbench_orders o2 ON o2.customer_id = c.id WHERE o2.id > o.id ORDER BY c.name",
           Some(2), &[(0, "name", "Ada"), (0, "other", "50")]));
        v.push(cc("lagInFrame",
           "SELECT id, amount - lagInFrame(toNullable(amount)) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) AS diff FROM dbench_orders ORDER BY id",
           Some(5), &[(0, "diff", "NULL"), (1, "diff", "-50")]));
        v.push(cc("UNION DISTINCT",
           "SELECT * FROM (SELECT country FROM dbench_customers WHERE country IS NOT NULL UNION DISTINCT SELECT 'FR' AS country) ORDER BY country",
           Some(3), &[(0, "country", "FR")]));
    }
    if d != Clickhouse {
        // ClickHouse: no correlated subqueries; LEFT JOIN fills defaults instead of NULL.
        v.push(cc("LEFT JOIN with no match",
           "SELECT c.name, COUNT(o.id) AS n FROM dbench_customers c LEFT JOIN dbench_orders o ON o.customer_id = c.id GROUP BY c.name ORDER BY n, c.name",
           Some(4), &[(0, "name", "Dee"), (0, "n", "0")]));
        v.push(cc("correlated scalar subquery",
           "SELECT name, (SELECT MAX(amount) FROM dbench_orders o WHERE o.customer_id = c.id) AS mx FROM dbench_customers c ORDER BY id",
           Some(4), &[(0, "mx", "100"), (3, "mx", "NULL")]));
        v.push(cc("NOT EXISTS",
           "SELECT name FROM dbench_customers c WHERE NOT EXISTS (SELECT 1 FROM dbench_orders o WHERE o.customer_id = c.id)",
           Some(1), &[(0, "name", "Dee")]));
    }
    v
}

async fn run_complex<D: Driver>(name: &'static str, driver: D, cfg: D::Config, d: Dialect) {
    let mut r = Report::new(name);
    let mut c = match driver.connect(&cfg).await {
        Ok(c) => c,
        Err(e) => {
            r.fail(format!("connect: {e}"));
            return r.finish();
        }
    };
    for s in complex_setup(d) {
        if let Err(e) = c.execute(&Query::new(&s)).await {
            r.fail(format!("setup `{}`: {e}", &s[..s.len().min(60)]));
        }
    }
    for case in complex_cases(d) {
        match c.execute(&Query::new(&case.sql)).await {
            Ok(res) => {
                if let Some(n) = case.rows {
                    r.check(
                        res.rows.len() == n,
                        format!("{}: {} rows, want {n}", case.what, res.rows.len()),
                    );
                }
                if let Some(n) = case.cols {
                    r.check(
                        res.columns.len() == n,
                        format!(
                            "{}: {} columns {:?}, want {n}",
                            case.what,
                            res.columns.len(),
                            res.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
                        ),
                    );
                }
                for (row, col, want) in &case.cells {
                    let got = res
                        .columns
                        .iter()
                        .position(|c| c.name.eq_ignore_ascii_case(col))
                        .and_then(|i| res.rows.get(*row).map(|r| &r.values[i]));
                    r.check(
                        got.is_some_and(|v| matches(v, want)),
                        format!(
                            "{}: row {row} `{col}` want {want:?}, got {got:?}",
                            case.what
                        ),
                    );
                }
            }
            Err(e) => r.fail(format!("{}: {e}", case.what)),
        }
    }
    r.finish();
}

#[tokio::test]
async fn complex_postgres() {
    if !enabled() {
        return;
    }
    let cfg = PostgresConfig {
        host: "localhost".into(),
        port: 5432,
        database: "postgres".into(),
        username: std::env::var("USER").unwrap_or_else(|_| "postgres".into()),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
        application_name: None,
    };
    run_complex("postgres complex", PostgresDriver, cfg, Dialect::Postgres).await;
}

#[tokio::test]
async fn complex_cockroachdb() {
    if !enabled() {
        return;
    }
    let cfg = PostgresConfig {
        host: "localhost".into(),
        port: 26257,
        database: "defaultdb".into(),
        username: "root".into(),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
        application_name: None,
    };
    run_complex(
        "cockroachdb complex",
        PostgresDriver,
        cfg,
        Dialect::Cockroach,
    )
    .await;
}

#[tokio::test]
async fn complex_mysql() {
    if !enabled() {
        return;
    }
    let cfg = MysqlConfig {
        host: "127.0.0.1".into(),
        port: 3306,
        database: "dbench_matrix".into(),
        username: "root".into(),
        password: None,
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
    };
    run_complex("mysql complex", MysqlDriver, cfg, Dialect::Mysql).await;
}

#[tokio::test]
async fn complex_sqlite() {
    if !enabled() {
        return;
    }
    let path = std::env::temp_dir().join("dbench_complex.db");
    let _ = std::fs::remove_file(&path);
    let cfg = SqliteConfig {
        path: path.to_string_lossy().into_owned(),
        mode: ConnectionMode::ReadWrite,
        wal_mode: false,
    };
    run_complex("sqlite complex", SqliteDriver, cfg, Dialect::Sqlite).await;
}

#[tokio::test]
async fn complex_mssql() {
    if !enabled() {
        return;
    }
    let cfg = MssqlConfig {
        password: Some("Dbench!2024".into()),
        tls: no_tls(),
        ..MssqlConfig::default()
    };
    run_complex("sql server complex", MssqlDriver, cfg, Dialect::Mssql).await;
}

#[tokio::test]
async fn complex_clickhouse() {
    if !enabled() {
        return;
    }
    let cfg = ClickhouseConfig {
        host: "localhost".into(),
        port: 8123,
        database: "default".into(),
        username: "default".into(),
        password: Some("dbench".into()),
        tls: no_tls(),
        mode: ConnectionMode::ReadWrite,
        connect_timeout_ms: Some(5000),
    };
    run_complex(
        "clickhouse complex",
        ClickhouseDriver,
        cfg,
        Dialect::Clickhouse,
    )
    .await;
}
