# Catalyst DBench

**The universal database IDE, built in Rust.**

DBench is a fast, open-source, cross-platform desktop app for connecting to, querying, and administering your databases (SQL, document, key-value, search and cloud) from one interface.

[![CI](https://github.com/hadihaider055/catalyst-dbench/actions/workflows/ci.yml/badge.svg)](https://github.com/hadihaider055/catalyst-dbench/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hadihaider055/catalyst-dbench)](https://github.com/hadihaider055/catalyst-dbench/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org)

---

## Contents

- [Why Catalyst DBench?](#why-catalyst-dbench)
- [Supported Databases](#supported-databases)
- [Installation](#installation)
- [Connecting](#connecting)
- [Features](#features)
- [Security](#security)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [Roadmap](#roadmap)
- [License](#license)

---

## Why Catalyst DBench?

Modern stacks are polyglot. You use Postgres in production, Redis for caching, MongoDB for documents, DynamoDB and Aurora on AWS. Every database ships its own GUI tool, each with a different UX, install process and set of shortcuts.

**DBench gives you one IDE for all of them.**

- One connection manager, one editor, one data grid, for 13 database engines and 40+ cloud services
- Native performance: Rust backend, Tauri shell, no Electron
- Secure by default: OS keychain, verified TLS, enforced read-only mode, tamper-evident audit log
- No telemetry. Fully open source.

---

## Supported Databases

### Native drivers

| Database                   | Status | Type           | Query language                      |
| -------------------------- | ------ | -------------- | ----------------------------------- |
| PostgreSQL                 | Stable | SQL            | SQL                                 |
| MySQL / MariaDB            | Stable | SQL            | SQL                                 |
| SQLite                     | Stable | SQL (embedded) | SQL                                 |
| CockroachDB                | Stable | SQL            | SQL (Postgres wire protocol)        |
| ClickHouse                 | Stable | OLAP           | SQL (HTTP API)                      |
| MongoDB                    | Stable | Document       | JSON commands (`find`, `aggregate`) |
| Redis                      | Stable | Key-Value      | Redis commands                      |
| Cassandra / ScyllaDB       | Stable | Wide-Column    | CQL                                 |
| SQL Server / Azure SQL     | Beta   | SQL            | T-SQL                               |
| Oracle                     | Beta   | SQL            | SQL / PL/SQL                        |
| Amazon DynamoDB            | Beta   | Key-Value      | PartiQL                             |
| Elasticsearch / OpenSearch | Beta   | Search         | SQL, or Dev Tools console syntax    |
| SurrealDB                  | Beta   | Multi-model    | SurrealQL                           |

### Cloud & compatible services

These speak the wire protocol of a native driver. Choose one from the **preset** dropdown in the New Connection dialog and it fills in the right driver, port and TLS setting.

| Service                                                                                 | Driver                     |
| --------------------------------------------------------------------------------------- | -------------------------- |
| **Amazon Aurora PostgreSQL**, Aurora DSQL, RDS PostgreSQL, Redshift                     | PostgreSQL                 |
| **Amazon Aurora MySQL**, RDS MySQL / MariaDB                                            | MySQL                      |
| Amazon RDS SQL Server, **Azure SQL Database**                                           | SQL Server                 |
| Amazon RDS Oracle, Oracle Autonomous Database                                           | Oracle                     |
| Amazon DocumentDB, Azure Cosmos DB (MongoDB API), MongoDB Atlas, FerretDB               | MongoDB                    |
| Amazon ElastiCache / MemoryDB, Azure Cache for Redis, Valkey, Dragonfly, KeyDB, Upstash | Redis                      |
| Amazon OpenSearch Service, Elastic Cloud                                                | Elasticsearch / OpenSearch |
| DynamoDB Local                                                                          | DynamoDB                   |
| Google Cloud SQL (Postgres / MySQL), AlloyDB                                            | PostgreSQL / MySQL         |
| Azure Database for PostgreSQL / MySQL                                                   | PostgreSQL / MySQL         |
| Supabase, Neon, Timescale, YugabyteDB, CockroachDB Cloud                                | PostgreSQL                 |
| PlanetScale, TiDB, SingleStore                                                          | MySQL                      |
| ScyllaDB                                                                                | Cassandra                  |
| ClickHouse Cloud                                                                        | ClickHouse                 |
| SurrealDB Cloud                                                                         | SurrealDB                  |

> Redshift and Cosmos DB implement only part of their upstream protocol. Queries work, but some schema-browser features may be limited.

---

## Installation

### Download

Get the installer for your OS from the [latest release](https://github.com/hadihaider055/catalyst-dbench/releases/latest):

| OS                    | File                                    |
| --------------------- | --------------------------------------- |
| macOS (Apple Silicon) | `Catalyst.DBench_<version>_aarch64.dmg` |
| macOS (Intel)         | `Catalyst.DBench_<version>_x64.dmg`     |
| Windows               | `.msi` or `-setup.exe`                  |
| Linux                 | `.AppImage`, `.deb` or `.rpm`           |

> Releases up to v1.4.0 were published without installers. If the latest release has no files attached, [build from source](#build-from-source).

The app isn't signed with a paid Apple or Microsoft certificate yet, so your OS will ask you to confirm it the first time you open it.

**macOS:** the first launch says _"cannot be opened because the developer cannot be verified"_. Right-click the app → **Open** → **Open**, or go to **System Settings → Privacy & Security → Open Anyway**. You only need to do this once. If you get _"is damaged and can't be opened"_ instead, clear the download flag:

```bash
xattr -cr "/Applications/Catalyst DBench.app"
```

**Windows:** if SmartScreen shows _"Windows protected your PC"_, click **More info → Run anyway**.

**Linux (AppImage):**

```bash
chmod +x Catalyst*.AppImage
./Catalyst*.AppImage
```

### Oracle Instant Client (Oracle only)

The Oracle driver loads Oracle's client libraries at runtime. Every other database works without them.

| OS      | Install                                                                                                                                                                       |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | Download **Instant Client Basic** from [Oracle](https://www.oracle.com/database/technologies/instant-client/downloads.html), unzip it, and copy the `.dylib` files to `~/lib` |
| Linux   | Install the Instant Client Basic package, then run `sudo ldconfig` (or set `LD_LIBRARY_PATH`)                                                                                 |
| Windows | Unzip Instant Client Basic and add the folder to your `PATH`                                                                                                                  |

If it's missing, connecting shows error `DPI-1047` along with a link to the download page.

### Build from source

**Prerequisites**

- [Rust](https://rustup.rs) 1.92 or newer (`rustup update`)
- [Node.js](https://nodejs.org) 20+
- Tauri CLI: `cargo install tauri-cli --version "^2" --locked`
- Platform dependencies:
  - **macOS:** `xcode-select --install`
  - **Linux (Debian/Ubuntu):** `sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libssl-dev patchelf`
  - **Windows:** [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and WebView2 (preinstalled on Windows 11)

**Run in development** (two terminals):

```bash
git clone https://github.com/hadihaider055/catalyst-dbench.git
cd catalyst-dbench

# Terminal 1: frontend (Vite on http://localhost:5173)
cd frontend && npm install && npm run dev

# Terminal 2: desktop app
cd crates/dbench-app && cargo tauri dev
```

**Build an installer:**

```bash
cd frontend && npm install && cd ..
cargo tauri build --project-dir crates/dbench-app
# Output: target/release/bundle/
```

### Try it with local databases

No database handy? Start one with Docker, then connect to `localhost` in the app:

```bash
docker run -d -p 5432:5432   -e POSTGRES_PASSWORD=postgres postgres:16
docker run -d -p 3306:3306   -e MYSQL_ROOT_PASSWORD=root mysql:8
docker run -d -p 27017:27017 mongo:7
docker run -d -p 6379:6379   redis:7
docker run -d -p 1433:1433   -e ACCEPT_EULA=Y -e MSSQL_SA_PASSWORD='Dbench!2024' mcr.microsoft.com/mssql/server:2022-latest
docker run -d -p 1521:1521   -e ORACLE_PASSWORD=oracle gvenzl/oracle-free:slim
docker run -d -p 9200:9200   -e discovery.type=single-node -e DISABLE_SECURITY_PLUGIN=true opensearchproject/opensearch:2
docker run -d -p 8000:8000   surrealdb/surrealdb:latest start --user root --pass root
docker run -d -p 8001:8000   amazon/dynamodb-local   # endpoint: http://localhost:8001
```

Local containers use self-signed certificates or none at all, so turn **TLS off** for them. SQLite needs nothing; just pick a `.db` file.

---

## Connecting

Most databases use the usual Host / Port / Database / Username / Password fields. For the rest, the dialog relabels the fields as shown below:

| Database                       | Host                                                                   | Port                     | Database field                          | Username / Password                                                               |
| ------------------------------ | ---------------------------------------------------------------------- | ------------------------ | --------------------------------------- | --------------------------------------------------------------------------------- |
| **SQL Server / Azure SQL**     | Server name                                                            | 1433                     | Database (optional)                     | SQL login                                                                         |
| **Oracle**                     | Hostname, or a full `(DESCRIPTION=…)` TNS descriptor                   | 1521 (TCPS: 1522 / 2484) | **Service name**, e.g. `FREEPDB1`       | Database user                                                                     |
| **DynamoDB**                   | **Region** (`us-east-1`), or an endpoint URL (`http://localhost:8000`) | n/a                      | Region (only when Host is an endpoint)  | Access key ID / secret. Leave both empty to use your AWS profile, env vars or SSO |
| **Elasticsearch / OpenSearch** | Hostname                                                               | 9200 (cloud: 443)        | Index pattern, e.g. `logs-*` (optional) | Basic auth, **or** leave username empty and paste an API key as the password      |
| **SurrealDB**                  | Hostname                                                               | 8000                     | `namespace/database`                    | Root, namespace or database user                                                  |
| **Redis**                      | Hostname                                                               | 6379                     | DB index                                | ACL user (optional) / password                                                    |

**Aurora, RDS and other AWS databases.** TLS certificates are verified, and AWS signs them with its own certificate authority, which your OS doesn't trust by default. Download the [AWS RDS CA bundle](https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem) and enter its path in **CA certificate** (shown when TLS is on). For **Aurora DSQL**, generate an IAM auth token and use it as the password.

**Query examples for the newer drivers:**

```sql
-- SQL Server: T-SQL
SELECT TOP 10 name, create_date FROM sys.tables ORDER BY create_date DESC;

-- Oracle: no trailing semicolon needed; PL/SQL blocks keep their END;
SELECT table_name FROM user_tables FETCH FIRST 10 ROWS ONLY

-- DynamoDB: PartiQL (quote table names; results are capped at 5,000 items)
SELECT * FROM "Orders" WHERE customer_id = 'c-42'

-- SurrealDB: SurrealQL
SELECT * FROM person WHERE age > 30 LIMIT 20;
```

```
# Elasticsearch / OpenSearch: Dev Tools console syntax (or plain SQL)
GET /logs-*/_search
{
  "size": 20,
  "query": { "match": { "level": "error" } }
}
```

---

## Features

- **Query editor:** Monaco-based, with syntax highlighting, autocomplete, per-database snippets and formatting (Postgres, MySQL, SQLite, T-SQL, PL/SQL)
- **Multi-statement batches:** run several statements at once and get a separate result set for each
- **Schema explorer:** tables, views, columns, keys, indexes and collections; MongoDB grouped by database
- **Data grid:** virtualized, with sorting, filtering, pagination, inline editing and row deletes (edits require a primary key)
- **ER diagram:** tables and foreign-key relationships, with drag, pan and zoom
- **Visual explain plan:** tree view of PostgreSQL / MySQL `EXPLAIN` output
- **Import / export:** import CSV/JSON into tables; export results as CSV or JSON, and schema as SQL DDL
- **Connection manager:** cloud presets, URI mode (MongoDB / Postgres / MySQL / Redis), SSH tunnels, database switcher
- **Tabs and history:** multiple connections side by side; full query history
- **Shortcuts:** `⌘/Ctrl+Enter` run, `⌘/Ctrl+T` new tab, `⌘/Ctrl+W` close tab, `⌘/Ctrl+B` sidebar, `⌘/Ctrl+/` list all shortcuts
- **Dark and light themes**

---

## Security

DBench handles production credentials and data, so it's secure by default:

- **Credentials:** passwords go to the OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service). They're never written to config files or browser storage, and they're redacted from logs and debug output.
- **TLS:** server certificates are always verified, against the OS trust store plus an optional CA bundle you provide. There's no "skip verification" switch.
- **SSH tunnels:** connect through a bastion host with your SSH agent or a key file.
- **Read-only mode:** per connection, enforced by the database server where it supports it (PostgreSQL/MySQL read-only sessions, ClickHouse `readonly=1`, SQLite read-only file mode). Every engine also checks each statement before sending it. The check ignores comments and string literals, so tricks like `/* … */ DROP TABLE` or `SELECT 1; DELETE …` are blocked.
- **Safe generated SQL:** identifiers and values that DBench writes into SQL (DDL viewer, grid edits) are quoted and escaped. Grid edits and deletes are never sent without a primary-key `WHERE` clause.
- **Audit log:** every connection and query is appended to a hash-chained JSONL log (macOS: `~/Library/Logs/dev.catalystdbench.app/audit.jsonl`). Queries are stored as SHA-256 hashes, not plaintext, and editing any entry breaks the chain.
- **No telemetry.**

Read more in [docs/security-architecture.md](docs/security-architecture.md). To report a vulnerability, see [SECURITY.md](SECURITY.md).

---

## Architecture

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

---

## Contributing

Contributions are very welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started; good first issues are labeled [`good first issue`](https://github.com/hadihaider055/catalyst-dbench/labels/good%20first%20issue).

### Testing

```bash
cargo test --workspace                                   # unit tests
cargo clippy --workspace --all-targets -- -D warnings    # lint (CI enforces)
cargo fmt --all                                          # format
```

Live smoke tests run the newer drivers against real servers. Start the containers listed in [`crates/dbench-drivers/tests/live.rs`](crates/dbench-drivers/tests/live.rs), then run:

```bash
DBENCH_LIVE_MSSQL='Dbench!2024' DBENCH_LIVE_SURREAL=1 DBENCH_LIVE_ES=1 DBENCH_LIVE_DYNAMO=1 \
  cargo test -p dbench-drivers --features all --test live -- --nocapture
```

### Adding a database

- **It speaks an existing protocol** (Postgres, MySQL, MongoDB, Redis, CQL, …): just add a preset to `PRESETS` in `frontend/src/components/ConnectionDialog/index.tsx`.
- **It needs a new driver:** follow the step-by-step guide in [CLAUDE.md](CLAUDE.md#adding-a-new-database-driver).

---

## Roadmap

| Status  | Focus                                                                               |
| ------- | ----------------------------------------------------------------------------------- |
| ✅ Done | PostgreSQL, MySQL, SQLite, MongoDB, Redis, ClickHouse, Cassandra, CockroachDB       |
| ✅ Done | SQL Server, Oracle, DynamoDB, Elasticsearch / OpenSearch, SurrealDB (beta)          |
| ✅ Done | SSH tunnel, ER diagram, explain plan, CSV/JSON import, batch results, cloud presets |
| Next    | Stabilize beta drivers, CA-certificate file picker in the connection dialog         |
| Later   | InfluxDB, signed & notarized builds, Homebrew cask                                  |

See [CHANGELOG.md](CHANGELOG.md) for release history.

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.

---

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
