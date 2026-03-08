# Catalyst DBench — CLAUDE.md

## Project Overview

**Catalyst DBench** is an open-source, cross-platform database IDE built in Rust. It supports connecting to, querying, and administering any major database (SQL, NoSQL, Key-Value, etc.) from a single desktop application.

- **Stack**: Rust (workspace), Tauri v2, React 19 + TypeScript, Monaco Editor
- **Goal**: Universal database IDE — fast, clean, open source
- **License**: MIT OR Apache-2.0
- **Status**: Milestones 0–4 complete. Features: SSH tunnel, ER diagram, multi-DB switcher, CSV/JSON import, visual explain plan, all 6 drivers.

## Repository Structure

```
catalyst/
├── Cargo.toml                    # Workspace root
├── README.md                     # Public-facing README
├── CONTRIBUTING.md               # Contribution guide
├── SECURITY.md                   # Security policy
├── CHANGELOG.md                  # Release history
├── .dev-notes/                   # Internal dev docs (gitignored)
│   ├── PLAN.md                   # Full project plan and roadmap
│   ├── PROGRESS.md               # Feature tracker
│   └── STATUS.md                 # Known issues and guidelines
├── crates/
│   ├── dbench-core/              # Core traits, types, errors
│   ├── dbench-macros/            # Procedural macros
│   ├── dbench-security/          # TLS, SSH tunnel, keychain, audit, crypto
│   ├── dbench-drivers/           # All database driver implementations
│   ├── dbench-engine/            # Query execution, connection registry
│   └── dbench-app/               # Tauri desktop app backend
├── frontend/                     # React + TypeScript UI (Vite)
│   ├── src/
│   │   ├── components/           # UI components (PascalCase directories)
│   │   ├── stores/               # Zustand state stores
│   │   └── lib/                  # Shared utilities, Tauri API wrappers
│   ├── package.json
│   └── vite.config.ts
└── .github/
    ├── workflows/                # CI/CD pipelines
    └── ISSUE_TEMPLATE/           # Issue templates
```

## Key Crates

### `dbench-core`
The heart of the project. Defines the traits that every database driver must implement.

- `src/driver.rs` — `Driver` trait
- `src/connection.rs` — `Connection` trait
- `src/query.rs` — `Query`, `QueryResult` types
- `src/schema.rs` — Schema introspection types
- `src/error.rs` — `CatalystError` (uses `thiserror`)
- `src/types.rs` — Shared value types (`Value` enum, `Row`, `Column`)

### `dbench-macros`
Procedural macro crate. Contains derive macros to reduce driver boilerplate.

- `#[derive(ConnectionConfig)]` — auto-implements `ConnectionConfig` trait, generates builder pattern, handles secret fields

### `dbench-drivers`
Houses all database driver implementations as modules. Each driver implements the `Driver` and `Connection` traits from `dbench-core`.

- `src/postgres/` — PostgreSQL via `tokio-postgres`
- `src/mysql/` — MySQL/MariaDB via `sqlx` (needs `chrono` feature for DateTime decode)
- `src/sqlite/` — SQLite via `rusqlite`
- `src/mongodb/` — MongoDB via official `mongodb` v2 crate
- `src/redis/` — Redis via `redis` v0.25
- `src/clickhouse/` — ClickHouse via HTTP API (port 8123, JSONCompact format)

**Critical driver notes:**
- MySQL `information_schema`: use `CAST(col AS CHAR)` — columns return as BLOB otherwise
- MongoDB v2: all collection/db methods require explicit `None` second arg (`find(filter, None)`, `aggregate(pipeline, None)`, etc.)
- Redis v0.25: `Value` enum variants are `Nil`, `Int(i64)`, `Data(Vec<u8>)`, `Bulk(Vec<Value>)`, `Status(String)`, `Okay` — NOT the newer `BulkString`/`Array` names

### `dbench-engine`
Higher-level runtime on top of drivers.

- `src/executor.rs` — Query execution with retry logic
- `src/registry.rs` — Driver registry (maps `DatabaseType` → `Box<dyn Driver>`)

### `dbench-security`
Security layer: TLS configuration, OS keychain integration, audit logging, SSH tunnel, AES-256-GCM crypto, PII masking.

- `src/tls.rs` — `TlsConfig`, `TlsMode` (Disabled / Preferred / Required)
- `src/keychain.rs` — `KeychainStore` wrapping the OS keychain (macOS Keychain, Secret Service on Linux, Windows Credential Store)
- `src/ssh.rs` — `SshTunnel` — real SSH port-forward via system `ssh` binary
- `src/audit.rs` — `AuditLogger` writing JSONL audit trail
- `src/crypto.rs` — AES-256-GCM encryption helpers
- `src/mask.rs` — PII masking for log output

### `dbench-engine`
Higher-level runtime on top of drivers.

- `src/executor.rs` — `QueryExecutor` with audit hooks
- `src/registry.rs` — `ConnectionRegistry` (DashMap, thread-safe)

### `dbench-app`
Tauri v2 backend. Defines all Tauri commands that the frontend calls via IPC.

- `src/main.rs` — App entry point, Tauri builder setup
- `src/commands/connections.rs` — connection management + SSH tunnel + `list_databases`
- `src/commands/query.rs` — `execute_query`, `execute_batch`
- `src/commands/schema.rs` — `get_schema`
- `src/commands/app.rs` — `get_version`, `get_audit_log_path`
- `src/state.rs` — `AppState` (registry, executor, audit, ssh_tunnels)

**Running the app (two terminals):**
```bash
# Terminal 1 — Frontend
cd frontend && npm run dev

# Terminal 2 — Backend
cd crates/dbench-app && cargo tauri dev
```
`beforeDevCommand` in `tauri.conf.json` is set to `""` — always start frontend separately.

## Frontend Architecture

### Key Components
| Component | Purpose |
|-----------|---------|
| `Layout/` | App shell, sidebar/results resize, tab bar, menu bar, welcome screen |
| `Sidebar/` | Connections + schema tree + saved queries + query history |
| `QueryEditor/` | Monaco editor, toolbar (Run/Explain/Format/font-size), transaction toolbar (SQL only), snippet bar |
| `ResultsGrid/` | TanStack Table: sort/filter, pagination, CSV/JSON export, copy row, cell expand modal, inline edit/delete |
| `ConnectionDialog/` | Add/edit/connect dialog — TLS, SSH tunnel, URI mode, keychain integration |
| `ERDiagram/` | SVG-based ER diagram with FK relationships, drag, pan, zoom |
| `ExplainPlan/` | Visual explain plan tree for PostgreSQL/MySQL EXPLAIN (FORMAT JSON) |
| `ImportDialog/` | CSV/JSON import — file picker, preview, batched INSERT |
| `StatusBar/` | Bottom bar with connection info, TLS badge, server version, row count |

### State (`useAppStore` — Zustand with persist middleware)
**Persisted to localStorage:**
- `savedConnections` — connection configs (no passwords)
- `history` — query history (newest first, max 500)
- `sidebarWidth`, `sidebarCollapsed`
- `editorFontSize` — Monaco editor font size (10–24, default 13)
- `theme` — "dark" | "light"

**Runtime only (not persisted):**
- `activeConnections` — backend UUID-keyed live connections
- `schemas` — schema cache: `DatabaseSchema | "loading" | { error: string }`
- `tabs`, `activeTabId`

**Critical ID invariant:** Backend generates the canonical connection UUID. After a successful `addConnection()` call, save `result.info.id` as the `SavedConnection.id`. This ensures `isActive()` checks and `loadSchema()` calls use the correct UUID.

## Development Commands

```bash
# Install dependencies
cd frontend && npm install

# Development (two terminals)
cd frontend && npm run dev
cd crates/dbench-app && cargo tauri dev

# Production build
cargo tauri build

# Tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all
```

## Code Style & Conventions

### Rust
- Edition: **2021**
- Error handling: `thiserror` for libraries, `anyhow` for binaries
- Async: **Tokio** (multi-thread)
- No `unwrap()` / `expect()` in library code — use `?`
- Logging: `tracing` macros (not `println!`)
- All public APIs need doc comments (`///`)

### TypeScript / React
- Strict TypeScript (`strict: true`)
- Functional components only
- Zustand for global state
- All Tauri IPC calls go through `src/lib/commands.ts`
- Tailwind for all styling — no inline styles, no CSS modules

### Git
- Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`
- Branch naming: `feat/feature-name`, `fix/bug-name`
- Squash merge into `main`

## Adding a New Database Driver

1. Add module under `crates/dbench-drivers/src/`
2. Create `Config` struct with `#[derive(ConnectionConfig)]`
3. Implement `Driver` + `Connection` traits from `dbench-core`
4. Register in `dbench-engine/src/registry.rs`
5. Add `DatabaseType` variant to `dbench-core/src/types.rs`
6. Wire in `crates/dbench-app/src/commands/connections.rs`
7. Add integration tests in `crates/dbench-drivers/tests/`
8. Update `PLAN.md` supported databases table

## Environment Variables

```bash
RUST_LOG=dbench=debug               # Verbose logging
WEBKIT_DISABLE_COMPOSITING_MODE=1   # Disable GPU compositing (VMs/CI)
```

## Important Design Decisions

1. **Tauri over Electron** — smaller binaries, Rust-native IPC, no Node.js overhead
2. **Trait-based drivers** — adding a DB requires only implementing `Driver` + `Connection`; no core changes
3. **Dual license MIT/Apache-2.0** — standard for Rust ecosystem
4. **No plain-text credentials** — passwords go to OS keychain, never to localStorage
5. **Backend UUID is canonical** — saved connection IDs must match backend UUIDs post-connect
6. **No telemetry by default** — opt-in only
