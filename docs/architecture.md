# Architecture

DBench is a **Tauri v2** desktop app:

- **Rust backend:** all database I/O, connection management, query execution and schema inspection
- **React 19 frontend:** Monaco editor, TanStack Table, Zustand state
- **Trait-based drivers:** supporting a new database means implementing two Rust traits, `Driver` and `Connection`

```
crates/
├── dbench-core       Core traits (Driver, Connection), types, read-only guard
├── dbench-macros     #[derive(ConnectionConfig)]: validation and secret redaction
├── dbench-security   TLS, SSH tunnel, OS keychain, audit log, crypto
├── dbench-drivers    One module per database (each behind a cargo feature)
├── dbench-engine     Query executor, connection registry
└── dbench-app        Tauri backend (IPC commands)
frontend/             React + TypeScript UI
```

## Testing

```bash
cargo test --workspace                                   # unit tests
cargo clippy --workspace --all-targets -- -D warnings    # lint (CI enforces)
cargo fmt --all                                          # format
```

Live smoke tests run the newer drivers against real servers. Start the containers listed in [`crates/dbench-drivers/tests/live.rs`](../crates/dbench-drivers/tests/live.rs), then run:

```bash
DBENCH_LIVE_MSSQL='Dbench!2024' DBENCH_LIVE_SURREAL=1 DBENCH_LIVE_ES=1 DBENCH_LIVE_DYNAMO=1 \
  cargo test -p dbench-drivers --features all --test live -- --nocapture
```

The type and query matrix checks every driver against real servers: each common column type reads back with its value (never a silent NULL), plus schema browsing, explain, batches, read-only blocking, error messages, and complex SQL (joins, subqueries, CTEs, window functions, set operations). Start the servers from the `docker run` lines in `live.rs` (plus local Postgres, MySQL and Redis), then:

```bash
DBENCH_MATRIX=1 cargo test -p dbench-drivers --features all --test matrix -- --nocapture --test-threads=1
```

## Adding a database

- **It speaks an existing protocol** (Postgres, MySQL, MongoDB, Redis, CQL, …): just add a preset to `PRESETS` in `frontend/src/components/ConnectionDialog/index.tsx`.
- **It needs a new driver:** follow the step-by-step guide in [CLAUDE.md](../CLAUDE.md#adding-a-new-database-driver).

## Acknowledgements

DBench builds on an excellent Rust ecosystem:

- [Tauri](https://tauri.app): desktop app framework
- [tokio-postgres](https://github.com/sfackler/rust-postgres): PostgreSQL
- [sqlx](https://github.com/launchbadge/sqlx): MySQL
- [rusqlite](https://github.com/rusqlite/rusqlite): SQLite
- [mongodb](https://github.com/mongodb/mongo-rust-driver): MongoDB
- [redis-rs](https://github.com/redis-rs/redis-rs): Redis
- [scylla-rust-driver](https://github.com/scylladb/scylla-rust-driver): Cassandra / ScyllaDB
- [tiberius](https://github.com/prisma/tiberius): SQL Server
- [rust-oracle](https://github.com/kubo/rust-oracle): Oracle
- [aws-sdk-rust](https://github.com/awslabs/aws-sdk-rust): DynamoDB
- [reqwest](https://github.com/seanmonstar/reqwest) and [rustls](https://github.com/rustls/rustls): HTTP and TLS
