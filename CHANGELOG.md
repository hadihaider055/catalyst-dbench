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
