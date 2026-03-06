# Catalyst DBench — Universal Database IDE, Built in Rust

> A fast, open-source, cross-platform database IDE that lets you connect, query, and administer any database from a single application.

---

## Vision

Modern developers work with multiple databases simultaneously — Postgres in production, SQLite locally, Redis for caching, MongoDB for documents, DynamoDB on AWS. Every database has its own tool, its own UI, its own quirks. **Catalyst DBench** eliminates that fragmentation.

One IDE. Every database. Built in Rust.

---

## Name & Branding

| Item | Value |
|------|-------|
| Product name | **Catalyst DBench** |
| CLI binary | `dbench` |
| Homebrew tap | `brew install dbench-io/tap/dbench` |
| GitHub org/repo | `dbench-io/dbench` |
| Tagline | *The universal database IDE, built in Rust* |

> Note: Verify `catalyst-db` GitHub org availability before publishing.

---

## Goals (v1.0)

- [ ] Connect to any major database via a unified interface
- [ ] Execute SQL/NoSQL queries with a rich editor (Monaco-based)
- [ ] Explore schema, tables, collections, indexes visually
- [ ] View and edit data in a performant data grid
- [ ] Manage multiple connections simultaneously (tabs)
- [ ] Export query results (CSV, JSON, Parquet)
- [ ] Cross-platform desktop app (macOS, Windows, Linux)
- [ ] Install via Homebrew on macOS/Linux

## Goals (v2.0 / SaaS)

- [ ] Team workspaces (shared connections, saved queries)
- [ ] Cloud sync for connection configs (encrypted)
- [ ] Query history across devices
- [ ] AI-powered query generation (Claude integration)
- [ ] Role-based access control
- [ ] Audit logging for enterprise
- [ ] Pricing: Free tier + Pro ($X/mo) + Team ($X/seat/mo) + Enterprise

---

## Supported Databases

### Phase 1 (v0.1 — MVP)
| Database | Type | Driver crate |
|----------|------|-------------|
| PostgreSQL | SQL | `tokio-postgres` |
| MySQL / MariaDB | SQL | `sqlx` (MySQL driver) |
| SQLite | SQL (embedded) | `rusqlite` |
| MongoDB | Document | `mongodb` (official) |
| Redis | Key-Value | `redis-rs` |

### Phase 2 (v0.2)
| Database | Type |
|----------|------|
| CockroachDB | SQL (Postgres-compatible) |
| DynamoDB | Key-Value / Document (AWS) |
| Cassandra / ScyllaDB | Wide-Column |
| ClickHouse | OLAP / Analytical |

### Phase 3 (v0.3+)
| Database | Type |
|----------|------|
| FaunaDB | Document |
| SurrealDB | Multi-model |
| TigerBeetle | Financial ledger |
| PlanetScale | MySQL-compatible cloud |
| Neon | Serverless Postgres |
| Turso (libSQL) | Edge SQLite |
| Supabase | Postgres + Auth |

---

## Technology Stack

### Core (Rust)
| Layer | Crate | Purpose |
|-------|-------|---------|
| `dbench-core` | (internal) | Traits, types, errors shared across all crates |
| `dbench-macros` | proc-macro crate | Derive macros for driver configs, query builders |
| `dbench-drivers` | (internal) | All database driver implementations |
| `dbench-engine` | (internal) | Connection pooling, query execution, result processing |
| `dbench-app` | Tauri v2 app | Desktop app backend (IPC commands, state) |

### UI (Frontend)
| Layer | Technology | Purpose |
|-------|-----------|---------|
| Framework | React 19 + TypeScript | Component UI |
| Build tool | Vite | Fast HMR dev server |
| Code editor | Monaco Editor | SQL/query editor with syntax highlighting |
| Data grid | TanStack Table v8 | High-performance table for query results |
| Styling | Tailwind CSS v4 | Utility-first CSS |
| State | Zustand | Lightweight global state |
| Icons | Lucide React | Clean icon set |

### Desktop
| Component | Technology |
|-----------|-----------|
| Desktop runtime | Tauri v2 |
| IPC | Tauri commands (type-safe Rust ↔ TypeScript) |
| Packaging | Tauri bundler (DMG, MSI, AppImage, .deb) |
| Auto-update | Tauri updater plugin |

---

## Architecture

```
┌────────────────────────────────────────────────────────┐
│                   Desktop Application                   │
│  ┌─────────────────────┐   ┌──────────────────────┐    │
│  │   React UI (Vite)   │   │  Tauri IPC Commands  │    │
│  │  Monaco + DataGrid  │◄──►  (type-safe bridge)  │    │
│  └─────────────────────┘   └──────────────────────┘    │
└────────────────────────────────────────────────────────┘
                          │
┌────────────────────────────────────────────────────────┐
│                   dbench-engine                       │
│  ┌───────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │  Connection   │  │    Query     │  │   Schema    │ │
│  │   Manager     │  │   Executor   │  │  Inspector  │ │
│  │ (pool/multi)  │  │              │  │             │ │
│  └───────────────┘  └──────────────┘  └─────────────┘ │
│  ┌───────────────┐  ┌──────────────┐                   │
│  │  Result Set   │  │   Export     │                   │
│  │  Processor    │  │   Engine     │                   │
│  └───────────────┘  └──────────────┘                   │
└────────────────────────────────────────────────────────┘
                          │
┌────────────────────────────────────────────────────────┐
│                  dbench-drivers                       │
│  ┌──────────┐ ┌───────┐ ┌────────┐ ┌──────┐ ┌───────┐ │
│  │ Postgres │ │ MySQL │ │ SQLite │ │Mongo │ │ Redis │ │
│  └──────────┘ └───────┘ └────────┘ └──────┘ └───────┘ │
│  ┌──────────┐ ┌───────┐ ┌────────┐ ┌──────┐           │
│  │CockroachDB│ │Dynamo │ │Cassandra│ │Click │           │
│  └──────────┘ └───────┘ └────────┘ └──────┘           │
└────────────────────────────────────────────────────────┘
                          │
┌────────────────────────────────────────────────────────┐
│                   dbench-core                         │
│  Driver trait · Connection trait · Query types          │
│  Schema types · Error types · Result types              │
│  dbench-macros: #[derive(ConnectionConfig)]           │
└────────────────────────────────────────────────────────┘
```

---

## Core Trait Design

### The `Driver` Trait
Every database implements `Driver`. This is the entry point for registering a new database.

```rust
pub trait Driver: Send + Sync + 'static {
    type Connection: Connection;
    type Config: ConnectionConfig;

    fn name(&self) -> &'static str;
    fn database_type(&self) -> DatabaseType;
    fn default_port(&self) -> Option<u16>;

    async fn connect(&self, config: &Self::Config) -> Result<Self::Connection>;
    async fn test_connection(&self, config: &Self::Config) -> Result<ConnectionInfo>;
}
```

### The `Connection` Trait
An active, live connection to a database instance.

```rust
pub trait Connection: Send + Sync {
    async fn execute(&mut self, query: &Query) -> Result<QueryResult>;
    async fn execute_batch(&mut self, queries: &[Query]) -> Result<Vec<QueryResult>>;
    async fn inspect_schema(&mut self) -> Result<DatabaseSchema>;
    async fn ping(&mut self) -> Result<Duration>;
    async fn close(self) -> Result<()>;
    fn is_alive(&self) -> bool;
    fn info(&self) -> &ConnectionInfo;
}
```

### The `#[derive(ConnectionConfig)]` Macro
Reduces boilerplate for driver configuration structs.

```rust
#[derive(ConnectionConfig, Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    #[config(secret)]
    pub password: Option<String>,
    pub ssl_mode: SslMode,
}
```

---

## UI Features (per database)

### SQL Databases (Postgres, MySQL, SQLite, etc.)
- Query editor with SQL syntax highlighting and auto-complete
- Table browser with column types, indexes, foreign keys
- Data viewer with filtering, sorting, pagination
- ER diagram view (schema visualization)
- Explain/analyze query plan visualization
- Transaction support (begin/commit/rollback)
- Saved queries (snippets)
- Query history

### Document Databases (MongoDB)
- Collection browser
- Document viewer/editor (JSON tree + raw)
- Aggregation pipeline builder
- Index management
- GridFS browser

### Key-Value (Redis)
- Key browser with type detection (String, List, Hash, Set, ZSet, Stream)
- TTL editor
- Pub/Sub monitor
- Memory analysis
- CLI passthrough mode

### Wide-Column (Cassandra, DynamoDB)
- Table/keyspace browser
- CQL/PartiQL editor
- Partition key explorer
- Capacity monitoring

---

## Open Source Strategy

### License
Dual license: **MIT OR Apache-2.0** (standard Rust open source)

### Community
- GitHub Issues for bug reports and feature requests
- GitHub Discussions for Q&A and ideas
- Contributing guide with clear PR process
- Semantic versioning (SemVer)
- Conventional Commits for changelog generation
- Good First Issue labels for new contributors

### Distribution
| Platform | Method |
|----------|--------|
| macOS | Homebrew tap, `.dmg` release |
| Linux | AppImage, `.deb`, `.rpm`, Snap, Flatpak |
| Windows | `.msi` installer, WinGet |
| All | Direct binary from GitHub Releases |
| All | `cargo install catalyst-db` (CLI mode) |

### CI/CD
- GitHub Actions: test, lint, build on push
- Release workflow: build for all platforms on tag push
- Code coverage with cargo-tarpaulin
- Security audit with cargo-audit
- Dependency review on PR

---

## Project Milestones

### Milestone 0 — Foundation (Current)
- [x] Workspace structure
- [x] Core traits (Driver, Connection)
- [x] Error types
- [x] Macros crate scaffold
- [ ] Tauri app scaffold
- [ ] React UI scaffold

### Milestone 1 — MVP (v0.1.0)
- [ ] PostgreSQL driver (connect, query, schema inspect)
- [ ] SQLite driver (connect, query, schema inspect)
- [ ] Basic query editor UI (Monaco)
- [ ] Basic data grid for results
- [ ] Connection manager (add/edit/delete connections)
- [ ] Schema sidebar

### Milestone 2 — Extended SQL (v0.2.0)
- [ ] MySQL / MariaDB driver
- [ ] CockroachDB driver (Postgres-compatible)
- [ ] Query history
- [ ] Export to CSV/JSON
- [ ] Dark/light theme

### Milestone 3 — NoSQL (v0.3.0)
- [ ] MongoDB driver
- [ ] Redis driver
- [ ] Document viewer UI
- [ ] Key-value browser UI

### Milestone 4 — Polish (v1.0.0)
- [ ] ER diagram view
- [ ] Query explain visualization
- [ ] Homebrew formula
- [ ] Comprehensive documentation
- [ ] Website (dbench.dev)

---

## Performance Targets
- Cold start: < 500ms
- Query result render (100k rows): < 200ms
- Schema load: < 1s for 500 tables
- Memory baseline: < 80MB

---

## Security Considerations
- Credentials encrypted at rest (OS keychain via `keyring` crate)
- No telemetry without explicit opt-in
- SSH tunnel support for remote databases
- TLS/SSL enforced by default
- Prepared statements only (no string interpolation in queries sent to DB)
- Connection configs stored locally; no cloud sync in v1

---

## SaaS Transition Plan (Future)
When moving to a SaaS model, the core desktop app remains open source (MIT/Apache). Paid features:
- **Pro**: Cloud sync, unlimited saved queries, AI assistance
- **Team**: Shared workspaces, team connections, query sharing
- **Enterprise**: SSO (SAML/OIDC), audit logs, on-prem deployment, SLA

The open source core is never paywalled. This is the "open core" model used by GitLab, Grafana, etc.
