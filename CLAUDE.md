# Catalyst DBench — CLAUDE.md

## Project Overview

**Catalyst DBench** is an open-source, cross-platform database IDE built in Rust. It supports connecting to, querying, and administering any major database (SQL, NoSQL, Key-Value, etc.) from a single desktop application.

- **Stack**: Rust (workspace), Tauri v2, React 19 + TypeScript, Monaco Editor
- **Goal**: Universal database IDE — fast, clean, open source
- **License**: MIT OR Apache-2.0

## Repository Structure

```
catalyst/
├── Cargo.toml                    # Workspace root
├── PLAN.md                       # Full project plan and roadmap
├── README.md                     # Public-facing README
├── CONTRIBUTING.md               # Contribution guide
├── crates/
│   ├── dbench-core/            # Core traits, types, errors
│   ├── dbench-macros/          # Procedural macros
│   ├── dbench-drivers/         # All database driver implementations
│   ├── dbench-engine/          # Query execution, connection pooling
│   └── dbench-app/             # Tauri desktop app backend
├── frontend/                     # React + TypeScript UI
│   ├── src/
│   │   ├── components/           # UI components
│   │   ├── views/                # Top-level views/pages
│   │   ├── stores/               # Zustand state stores
│   │   ├── hooks/                # React hooks
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
- `#[derive(SchemaMapper)]` — maps DB-specific schema types to Catalyst DBench's unified schema types

### `dbench-drivers`
Houses all database driver implementations as modules. Each driver implements the `Driver` and `Connection` traits from `dbench-core`.

- `src/postgres/` — PostgreSQL via `tokio-postgres`
- `src/mysql/` — MySQL/MariaDB via `sqlx`
- `src/sqlite/` — SQLite via `rusqlite`
- `src/mongodb/` — MongoDB via official `mongodb` crate
- `src/redis/` — Redis via `redis` crate

### `dbench-engine`
Higher-level runtime on top of drivers.

- `src/pool.rs` — Connection pool manager (wraps `bb8` or `deadpool`)
- `src/executor.rs` — Query execution with retry logic
- `src/inspector.rs` — Schema inspection abstraction
- `src/registry.rs` — Driver registry (maps `DatabaseType` → `Box<dyn Driver>`)

### `dbench-app`
Tauri v2 backend. Defines all Tauri commands that the frontend calls via IPC.

- `src/main.rs` — App entry point, Tauri builder setup
- `src/commands/` — All `#[tauri::command]` functions
- `src/state.rs` — `AppState` (connection registry, engine instance)

## Development Commands

```bash
# Install Rust toolchain
rustup toolchain install stable
rustup component add clippy rustfmt

# Install Tauri CLI
cargo install tauri-cli --version "^2"

# Install frontend deps
cd frontend && npm install

# Run in development mode (hot-reload)
cargo tauri dev

# Build for production
cargo tauri build

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all

# Security audit
cargo audit

# Check for unused dependencies
cargo machete
```

## Code Style & Conventions

### Rust
- Rust edition: **2021**
- Error handling: `thiserror` for library errors, `anyhow` for binaries/app code
- Async runtime: **Tokio** (multi-thread)
- Serialization: **serde** with `serde_json` and `serde_repr`
- All public APIs must have doc comments (`///`)
- Use `#[must_use]` on types/methods where ignoring the result is a bug
- Prefer `impl Trait` in function arguments, concrete types in return positions
- No `unwrap()` or `expect()` in library code — propagate errors with `?`
- Use `tracing` for structured logging (not `println!`)

### TypeScript / React
- Strict TypeScript (`strict: true`)
- Functional components only, no class components
- Zustand for global state
- All Tauri command calls go through `src/lib/commands.ts` (typed wrappers)
- Tailwind for all styling — no inline styles, no CSS modules

### Git
- Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`
- Branch naming: `feat/feature-name`, `fix/bug-name`, `chore/task-name`
- PRs require at least one review before merge
- Squash merge into `main`

## Adding a New Database Driver

1. Add a new module under `crates/dbench-drivers/src/`
2. Create `Config` struct using `#[derive(ConnectionConfig)]`
3. Implement `Driver` and `Connection` traits from `dbench-core`
4. Register the driver in `dbench-engine/src/registry.rs`
5. Add a `DatabaseType` variant to `dbench-core/src/types.rs`
6. Add integration tests under `crates/dbench-drivers/tests/`
7. Update `PLAN.md` supported databases table
8. Document the driver in `docs/drivers/`

## Testing Strategy

- **Unit tests**: in `src/` files (pure logic, no I/O)
- **Integration tests**: in `tests/` directories, require live DB (use Docker)
- **E2E tests**: Playwright for UI flows
- Use `testcontainers-rs` for spinning up databases in CI
- Coverage target: ≥ 80% for `dbench-core` and `dbench-engine`

## Environment Variables (Development)

```bash
# Enable verbose logging
RUST_LOG=dbench=debug

# Override config directory
DBENCH_CONFIG_DIR=~/.config/catalyst

# Disable hardware acceleration (for VMs)
WEBKIT_DISABLE_COMPOSITING_MODE=1
```

## Important Design Decisions

1. **Tauri over Electron**: smaller binaries, better performance, Rust-native IPC
2. **Trait-based drivers**: adding a new DB requires only implementing two traits — no changes to core
3. **Dual license MIT/Apache-2.0**: standard for Rust ecosystem, maximally permissive
4. **Credentials in OS keychain**: never stored in plain text config files
5. **No telemetry by default**: opt-in only, transparent about what is collected
6. **Tokio async throughout**: no blocking I/O on async executors
