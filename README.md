# Catalyst DBench

**The universal database IDE, built in Rust.**

DBench is a fast, open-source, cross-platform desktop application for connecting to, querying, and administering any database — from a single unified interface.

[![CI](https://github.com/dbench-io/dbench/actions/workflows/ci.yml/badge.svg)](https://github.com/dbench-io/dbench/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

---

## Why Catalyst DBench?

Modern stacks are polyglot. You use Postgres in production, Redis for caching, MongoDB for documents, DynamoDB on AWS. Every database ships with its own GUI tool — each with a different UX, different install process, different shortcut keys.

**DBench gives you one IDE for all of them.**

- Zero config switching between databases
- Consistent UI and keyboard shortcuts across every DB type
- Native performance — built in Rust, powered by Tauri
- Fully open source, forever free for individuals

---

## Supported Databases

| Database | Status | Type |
|----------|--------|------|
| PostgreSQL | ✅ Working | SQL |
| MySQL / MariaDB | ✅ Working | SQL |
| SQLite | ✅ Working | SQL (embedded) |
| MongoDB | ✅ Working | Document |
| Redis | ✅ Working | Key-Value |
| ClickHouse | ✅ Working | OLAP |
| CockroachDB | ✅ Working (Postgres-compatible) | SQL |
| DynamoDB | Planned (v0.2) | Key-Value |
| Cassandra / ScyllaDB | Planned (v0.3) | Wide-Column |
| FaunaDB | Planned (v0.3) | Document |
| SurrealDB | Planned (v0.3) | Multi-model |

---

## Features

- **Query Editor** — Monaco-based editor with SQL/NoSQL syntax highlighting, auto-complete, snippet bar, and formatting
- **Schema Explorer** — Browse tables, collections, indexes, and relationships; MongoDB grouped by database (Atlas-style)
- **Data Grid** — High-performance table with sorting, filtering, pagination, inline editing, and delete for both SQL and NoSQL
- **Connection Manager** — Securely store and manage connections (credentials in OS keychain); URI support for MongoDB/Postgres/Redis
- **Multiple Tabs** — Work across multiple connections simultaneously; tabs show table/collection name
- **App Menu Bar** — File / View / Tools dropdown menus (MongoDB Compass-style)
- **Keyboard Shortcuts** — Ctrl+T new tab, Ctrl+W close tab, Ctrl+Shift+W close all, Ctrl+B sidebar, Ctrl+/ shortcuts modal
- **Query History** — Full history of executed queries including inline edits
- **Export** — Export results to CSV or JSON; export schema as SQL DDL
- **Import SQL** — Open `.sql` files directly into the query editor
- **MongoDB Support** — JSON query mode (`find`, `aggregate`, `update`, `delete`, `insert`), Atlas-style tree, explain plans
- **ClickHouse Support** — HTTP API driver with full schema introspection
- **Dark / Light theme** — Toggle from menu or keyboard shortcut
- **Zoom** — Ctrl+= / Ctrl+- zoom in/out without breaking editor cursor placement

---

## Installation

> Catalyst DBench is in early development. Pre-built binaries will be available at v0.1.0.

### macOS (Homebrew)
```bash
brew tap catalyst-db/tap
brew install catalyst
```

### Linux (AppImage)
```bash
# Download from GitHub Releases
chmod +x CatalystDBench-*.AppImage
./CatalystDBench-*.AppImage
```

### Windows
Download the `.msi` installer from [GitHub Releases](https://github.com/dbench-io/dbench/releases).

### Build from Source
```bash
# Prerequisites: Rust 1.75+, Node.js 20+, Tauri v2 system deps

git clone https://github.com/dbench-io/dbench.git
cd dbench

# Install frontend dependencies
cd frontend && npm install && cd ..

# Install Tauri CLI
cargo install tauri-cli --version "^2"

# Run in development
cargo tauri dev

# Build production binary
cargo tauri build
```

---

## Architecture

DBench is a **Tauri v2** desktop application with:

- **Rust backend**: All database logic, connection pooling, query execution, schema inspection
- **React frontend**: UI built with React 19, Monaco Editor, and TanStack Table
- **Trait-based driver system**: Adding support for a new database means implementing two Rust traits

```
dbench-core        — Core traits (Driver, Connection) and types
dbench-macros      — Derive macros for driver boilerplate
dbench-drivers     — Database driver implementations
dbench-engine      — Query execution, connection pooling, schema inspection
dbench-app         — Tauri application backend (IPC commands)
frontend/            — React + TypeScript UI
```

See [PLAN.md](PLAN.md) for the full architecture and roadmap.

---

## Contributing

Contributions are very welcome! DBench is an ambitious open-source project and there is a lot to build.

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to get started. Good first issues are labeled [`good first issue`](https://github.com/dbench-io/dbench/labels/good%20first%20issue).

### Adding a New Database Driver

See [CLAUDE.md](CLAUDE.md#adding-a-new-database-driver) for a step-by-step guide.

---

## Roadmap

See [PLAN.md](PLAN.md) for the full roadmap.

| Version | Focus |
|---------|-------|
| v0.1.0 | PostgreSQL, SQLite, MongoDB, Redis — MVP |
| v0.2.0 | MySQL, CockroachDB, DynamoDB — Extended support |
| v0.3.0 | Cassandra, ClickHouse, FaunaDB, SurrealDB |
| v1.0.0 | ER diagrams, query explain UI, Homebrew, website |

---

## License

Catalyst DBench is dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE). You may use it under the terms of either license.

---

## Acknowledgements

DBench stands on the shoulders of an incredible Rust ecosystem:

- [Tauri](https://tauri.app) — desktop app framework
- [tokio-postgres](https://github.com/sfackler/rust-postgres) — PostgreSQL driver
- [mongodb](https://github.com/mongodb/mongo-rust-driver) — MongoDB driver
- [redis-rs](https://github.com/redis-rs/redis-rs) — Redis driver
- [sqlx](https://github.com/launchbadge/sqlx) — async SQL toolkit
- [serde](https://serde.rs) — serialization
- [thiserror](https://github.com/dtolnay/thiserror) — error handling
- [tracing](https://github.com/tokio-rs/tracing) — structured logging
