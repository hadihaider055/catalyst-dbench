# Catalyst DBench

**One fast, open-source desktop IDE for all your databases.** Built in Rust with Tauri.

[![CI](https://github.com/hadihaider055/catalyst-dbench/actions/workflows/ci.yml/badge.svg)](https://github.com/hadihaider055/catalyst-dbench/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hadihaider055/catalyst-dbench)](https://github.com/hadihaider055/catalyst-dbench/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

![Query editor and results](docs/screenshots/editor.png)

- **13 databases, one app:** PostgreSQL, MySQL, SQLite, SQL Server, Oracle, MongoDB, Redis, DynamoDB, Elasticsearch / OpenSearch, ClickHouse, Cassandra, CockroachDB and SurrealDB, plus presets for Aurora, RDS, Supabase and [40+ cloud services](docs/databases.md)
- **Native and light:** Rust backend, no Electron
- **Secure by default:** passwords in the OS keychain, verified TLS, enforced read-only mode, audit log
- **No telemetry**

## Install

Download the installer for your OS from the [latest release](https://github.com/hadihaider055/catalyst-dbench/releases/latest): `.dmg` for macOS (`aarch64` for Apple Silicon, `x64` for Intel), `.msi` / `-setup.exe` for Windows, `.AppImage` / `.deb` / `.rpm` for Linux.

The builds aren't code-signed yet. On macOS, right-click the app → **Open** the first time. See [installation](docs/installation.md) for Windows/Linux notes, Oracle, and building from source.

## Screenshots

| Schema browser and data grid | ER diagram |
| --- | --- |
| ![Data grid](docs/screenshots/grid.png) | ![ER diagram](docs/screenshots/er-diagram.png) |
| **New connection** | **Explain plan** |
| ![Connection dialog](docs/screenshots/connection.png) | ![Explain plan](docs/screenshots/explain.png) |
| **Dark theme** | |
| ![Dark theme](docs/screenshots/dark.png) | |

## Features

- Monaco editor with autocomplete, snippets, formatting and multi-statement batches
- Schema explorer, ER diagram and visual `EXPLAIN` plans
- Data grid with sorting, filtering, inline editing, and CSV/JSON import and export
- SSH tunnels, connection URIs, cloud presets and a database switcher
- Tabs, query history, dark and light themes

## Docs

- [Supported databases](docs/databases.md)
- [Installation and building from source](docs/installation.md)
- [Connecting](docs/connecting.md): per-database fields, AWS certificates, query examples
- [Security](docs/security-architecture.md)
- [Architecture and testing](docs/architecture.md)
- [Changelog](CHANGELOG.md)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md); to report a vulnerability, see [SECURITY.md](SECURITY.md).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
