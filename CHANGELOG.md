## [1.5.3](https://github.com/hadihaider055/catalyst-dbench/compare/v1.5.2...v1.5.3) (2026-10-07)

### Bug Fixes

* **security:** keep uri passwords in keychain, not in saved connections ([07435f6](https://github.com/hadihaider055/catalyst-dbench/commit/07435f6f5e063e6c6e79ec6df693914a8096c43e))

## [1.5.2](https://github.com/hadihaider055/catalyst-dbench/compare/v1.5.1...v1.5.2) (2026-10-07)

### Bug Fixes

* **ci:** fix cargo-deny-action v2 inputs ([b3f9a1a](https://github.com/hadihaider055/catalyst-dbench/commit/b3f9a1a72fda9a61ebfd383c55be94c3d9752d1f))

## [1.5.1](https://github.com/hadihaider055/catalyst-dbench/compare/v1.5.0...v1.5.1) (2026-10-07)

### Bug Fixes

* release builds, CI security checks and connection dialog ([6beef60](https://github.com/hadihaider055/catalyst-dbench/commit/6beef609840776f560716aa65fddba1eb808ac02))

## [1.5.0](https://github.com/hadihaider055/catalyst-dbench/compare/v1.4.0...v1.5.0) (2026-10-07)

### Features

* db drivers upgraded and semantic release for apple ([30c7a9f](https://github.com/hadihaider055/catalyst-dbench/commit/30c7a9f5c3761dd657ad25d584dd2ddb90fa24e9))

## [1.4.0](https://github.com/Catalyst-DBench/catalyst-dbench/compare/v1.3.0...v1.4.0) (2026-04-10)

### Features

* handling multiple line statements as batch results ([d2dfe9a](https://github.com/Catalyst-DBench/catalyst-dbench/commit/d2dfe9af33d64adc82504aef0f20ab4dd1a2eb74))

## [1.3.0](https://github.com/Catalyst-DBench/catalyst-dbench/compare/v1.2.0...v1.3.0) (2026-03-29)

### Features

* **mongodb:** table treeview display with tanstack virtualizer ([6f3aba3](https://github.com/Catalyst-DBench/catalyst-dbench/commit/6f3aba3ff44a2429f90119796598fab80c69729d))

## [1.2.0](https://github.com/hadihaider055/catalyst-dbench/compare/v1.1.0...v1.2.0) (2026-03-10)

### Features

* **drivers:** add Cassandra driver and multi-database switcher ([93a06ac](https://github.com/hadihaider055/catalyst-dbench/commit/93a06ac13e1926a658c0736bfb3173c23b07eda8))
* **ui:** toast system, DDL viewer, explain timing bars, hex dump, file open/save, NoSQL menu guard ([5e35074](https://github.com/hadihaider055/catalyst-dbench/commit/5e3507427e45a6d049a37d73af06915b886536d9))

## [1.1.0](https://github.com/hadihaider055/catalyst-dbench/compare/v1.0.0...v1.1.0) (2026-03-08)

### Features

* SSH tunnels, ER diagram, explain plan, CSV/JSON import, FK introspection ([fb84353](https://github.com/hadihaider055/catalyst-dbench/commit/fb843531325ca733f6f0492f0a100012398262db))

## 1.0.0 (2026-03-07)

### Features

* **app:** Tauri v2 IPC commands — connections, query, schema, app info ([1f11d8d](https://github.com/hadihaider055/catalyst-dbench/commit/1f11d8d3c4310e0ae833b758783acb82b76acc6f))
* **app:** wire postgres and sqlite to ipc commands ([a44fc3c](https://github.com/hadihaider055/catalyst-dbench/commit/a44fc3c29cedbac6201f5c3ae895c95df6e4a98a))
* **core:** Driver + Connection traits, Query/Result/Schema types, DatabaseType enum (19 databases), CatalystError ([42a3852](https://github.com/hadihaider055/catalyst-dbench/commit/42a3852649175ac00ed99a7f7d082d317e1e6c03))
* **drivers:** PostgreSQL, SQLite, MongoDB, Redis driver scaffolds ([529d855](https://github.com/hadihaider055/catalyst-dbench/commit/529d855a615fb861c04d2fad992f12bbbccd4e28))
* **drivers:** real sqlite impl, fix driver imports ([2356272](https://github.com/hadihaider055/catalyst-dbench/commit/2356272ab969155d35eea390790218be71940615))
* **engine:** connection adapter and real query execution ([f97b550](https://github.com/hadihaider055/catalyst-dbench/commit/f97b5506e498fe40495630af7018667e0093237c))
* **engine:** ConnectionRegistry (DashMap), QueryExecutor with audit hooks ([60442c8](https://github.com/hadihaider055/catalyst-dbench/commit/60442c8f19e9042fc49a236420594621e992eb93))
* **macros:** derive(ConnectionConfig) — secret field handling ([672ecbb](https://github.com/hadihaider055/catalyst-dbench/commit/672ecbbea1fee83a33e64aa4901965461f4b4019))
* **ui:** menu bar, tabs, MongoDB edit/explain, ClickHouse, import SQL, zoom fix ([08a523f](https://github.com/hadihaider055/catalyst-dbench/commit/08a523f4c0d059ff0977e0f3311b80846b7f0857))
* **ui:** React 19 + Monaco + TanStack Table + Zustand + Tailwind ([068c921](https://github.com/hadihaider055/catalyst-dbench/commit/068c9214b5005a534abdd9db4ce9d616e2450740))

### Bug Fixes

* **core:** fix ConnectionConfig export and error format ([504b5d8](https://github.com/hadihaider055/catalyst-dbench/commit/504b5d88ce7181060ce1e4ffdaada8967a5b3400))
* **security:** base64 and keyring v2 api fixes ([e01b95b](https://github.com/hadihaider055/catalyst-dbench/commit/e01b95b92821d50e3f64376c3717d2b244e71802))
* **ui:** typescript errors and updated ipc types ([9dd9250](https://github.com/hadihaider055/catalyst-dbench/commit/9dd9250dc8dbe207a96f0f29fe230611c8d27373))

### Security

* OS keychain, AES-256-GCM, audit log, TLS, SSH, PII masking ([0ddf6d8](https://github.com/hadihaider055/catalyst-dbench/commit/0ddf6d81c8eb473cb20a67ccf5757ec1629f03cd))

### Documentation

* initial project foundation, plan, and open source docs ([aae077a](https://github.com/hadihaider055/catalyst-dbench/commit/aae077a9153b78206b37fba42f873ff67f428971))
* **security:** add 7-layer security architecture documentation ([a23537a](https://github.com/hadihaider055/catalyst-dbench/commit/a23537a0d66361587c1d6429900349d02bff57ff))

# Changelog

All notable changes to Catalyst DBench are documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased] — 2026-03-10

### Added

#### UI / Frontend
- **App menu bar** — File, View, Tools dropdown menus (MongoDB Compass-style), replacing the plain title area
- **Two-row header** — Row 1: menu bar with drag region; Row 2: tab strip with New/Query buttons and Close All
- **Keyboard shortcuts** — Ctrl+T new tab, Ctrl+W close tab, Ctrl+Shift+W close all, Ctrl+B sidebar, Ctrl+/ shortcuts modal, Escape closes modals
- **Shortcuts modal** — Full list of keyboard shortcuts accessible from Tools menu or Ctrl+/
- **Tab titles** — Tabs now show table/collection name instead of "Query 1"
- **Close All Tabs** button in tab strip (also in File menu and Ctrl+Shift+W)
- **Import SQL File** — File > Import SQL File… opens native file picker, loads content into active tab
- **Export Schema as SQL** — File > Export Schema as SQL… generates CREATE TABLE DDL from cached schema
- **MongoDB inline edit/delete** — Edit mode now works for MongoDB collections; generates `update`/`delete` JSON commands
- **MongoDB inline edit history** — Inline edits and deletes are recorded in the query history tab
- **Explain support for MongoDB** — Wraps `find`/`aggregate` in `explain` command, shows execution stats plan
- **MongoDB column types** — Result grid now shows proper column types (ObjectId, String, Int32, Boolean, DateTime, etc.) instead of "bson"
- **ClickHouse driver** — Full HTTP API driver with JSONCompact format, schema introspection via `system.columns`/`system.tables`
- **ClickHouse snippets** — Query snippet bar includes SHOW TABLES, DESCRIBE TABLE, system.parts size queries
- **MongoDB Atlas-style tree** — Collections grouped by database with expandable database headers
- **MongoDB JSON query mode** — Editor uses JSON language mode for MongoDB tabs (no more red underlines)
- **Context menu zoom fix** — Context menus now correctly position under the cursor at any zoom level
- **Zoom fix for Monaco** — Zoom now uses CSS `transform: scale()` on the root container instead of `body.style.zoom`, fixing cursor click positioning in Monaco Editor
- **Welcome screen keychain** — Saved connections on the Welcome screen now pre-fetch password from OS keychain before connecting
- **Reconnect dialog** — On failed connect, the connection dialog opens pre-filled with saved credentials
- **Tab db type icon** — Tab buttons show a small database type icon prefix

#### Backend (Rust)
- **MongoDB `update` command** — Driver now handles `{"update": "col", "updates": [...]}` JSON queries
- **MongoDB `delete` command** — Driver now handles `{"delete": "col", "deletes": [...]}` JSON queries
- **MongoDB ObjectId coercion** — Filter documents automatically coerce 24-char hex strings to ObjectId for `_id` matching
- **MongoDB explain** — `execute` checks `query.explain` and wraps operation in `{ explain: ..., verbosity: "executionStats" }`
- **MongoDB db override** — All operations (find, aggregate, update, delete, insert) respect `"db"` field to target any database
- **`store_credential` / `get_credential`** IPC commands backed by OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- **`remove_connection`** now deletes the stored credential from the OS keychain
- **ClickHouse driver** — Full `Driver` + `Connection` implementation using `reqwest` HTTP client
- **`@tauri-apps/plugin-fs`** installed and registered for SQL file import

### Fixed
- MongoDB context menu "Select rows" was generating SQL instead of a JSON find command
- MongoDB `_id` filter using string instead of ObjectId caused update/delete to match nothing
- Monaco Editor cursor click position offset at zoom != 100% (CSS `body.zoom` incompatible with WebKit)
- Keyboard shortcut Escape not closing modals (was behind `if (!mod) return` guard)
- Ctrl+? / Cmd+? shortcut not opening shortcuts modal (key is `"?"` which requires Shift; now also handles `"/"`)
- Welcome screen MySQL/Postgres connect not sending keychain password
- MongoDB editor showing red underlines due to using `javascript` language mode (now uses `json`)
- MongoDB query history entries missing for inline edits in ResultsGrid

---

## [0.1.0-alpha] — 2026-02-01

### Added
- Initial project scaffold: 6-crate Rust workspace
- Tauri v2 desktop app with React 19 + TypeScript frontend
- Monaco Editor query editor with SQL syntax highlighting and auto-complete
- TanStack Table v8 data grid with sorting, filtering, pagination
- PostgreSQL driver via `tokio-postgres`
- MySQL/MariaDB driver via `sqlx`
- SQLite driver via `rusqlite`
- MongoDB driver via official `mongodb` crate
- Redis driver via `redis-rs`
- 7-layer security: OS keychain, AES-256-GCM, audit log, TLS, masking
- Connection registry and query executor
- Schema introspection for all drivers
- Schema sidebar with table/collection tree and context menus
- Query history (newest-first, capped at 500)
- Saved queries
- CSV/JSON export
- Dark/light theme toggle
- Zoom controls (menu bar + keyboard)
- Status bar with connection info
