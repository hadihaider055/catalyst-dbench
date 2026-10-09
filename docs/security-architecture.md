# Security Architecture — Catalyst DBench

This document describes the complete security model for Catalyst DBench. Every layer is implemented in the `dbench-security` crate.

---

## Threat Model

DBench is a local desktop application that holds credentials to production databases. The threats it protects against:

| Threat                       | Mitigation                               |
| ---------------------------- | ---------------------------------------- |
| Stolen credentials from disk | OS keychain; encrypted config            |
| Credentials leaked in logs   | `secrecy::Secret<T>` type; log filtering |
| Credentials in memory dumps  | `zeroize` — zero memory on drop          |
| MITM on DB connection        | TLS enforced; cert validation            |
| Remote host compromise       | SSH tunneling; key-based auth            |
| Malicious query injection    | Parameterized queries only               |
| UI XSS → IPC escalation      | Tauri CSP; strict IPC allowlist          |
| Unauthorized DB writes       | Read-only connection mode                |
| Tampered audit logs          | Append-only; cryptographic chaining      |
| Secrets in exported data     | Column masking; export confirmation      |

---

## Layer 1 — Credential Security

### Storage

All database credentials are stored **exclusively** in the OS keychain:

| OS      | Backend                                      |
| ------- | -------------------------------------------- |
| macOS   | Keychain Services (via `security` framework) |
| Windows | Windows Credential Manager                   |
| Linux   | Secret Service API (GNOME Keyring / KWallet) |

The `keyring` crate provides a cross-platform abstraction. Each credential is stored under a namespaced key: `catalyst/<connection_id>/password`.

**Connection config files** (stored in `~/.config/catalyst/connections/`) contain everything _except_ the password. They reference the keychain entry by connection ID.

### Secret<T> Type

All credential values are wrapped in `secrecy::Secret<T>`:

```rust
use secrecy::{Secret, ExposeSecret};

pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: Secret<String>,   // never accidentally logged or cloned
}
```

`Secret<T>` has no `Display`, no `Debug` output of the value, and `Clone` is disabled. You must explicitly call `.expose_secret()` at the exact point of use.

### Config File Encryption

Connection config files that contain sensitive metadata (SSH private key paths, client cert paths) are encrypted with AES-256-GCM using a key derived from a machine-specific secret (hardware UUID + OS username, via PBKDF2).

---

## Layer 2 — Transport Security

### TLS

- TLS 1.2 minimum enforced; TLS 1.3 preferred
- Certificate validation **on by default**; disabling shows a prominent warning and requires typing "I understand the risks"
- Uses `rustls` (pure Rust) where possible; falls back to native TLS via `native-tls` where required by the DB driver
- SNI (Server Name Indication) enabled by default

### SSH Tunneling

Many production databases are not exposed directly. Catalyst DBench supports SSH tunneling:

```
  Catalyst DBench App
       │
       │  SSH encrypted tunnel (port forwarding)
       ▼
  Bastion / Jump Host
       │
       │  Internal network
       ▼
  Database Server
```

Supported authentication:

- SSH key (Ed25519, RSA) — keys read from filesystem, passphrase via keychain
- SSH agent forwarding
- Password (discouraged, stored in keychain)

Implementation: `ssh2` crate (libssh2 bindings).

### Certificate Pinning

For enterprise use cases, certificate pinning can be configured per-connection to prevent MITM even with a compromised CA.

---

## Layer 3 — Memory Security

### Zeroize on Drop

All types holding sensitive values implement `Zeroize` and `ZeroizeOnDrop`:

```rust
use zeroize::ZeroizeOnDrop;

#[derive(ZeroizeOnDrop)]
pub struct PlaintextCredential {
    value: String,   // zeroed (0x00) when this struct is dropped
}
```

This ensures credentials do not linger in heap memory after use.

### No Sensitive Values in Logs

The `tracing` subscriber is configured with a `SensitiveFilter` layer that scrubs known sensitive field names (`password`, `token`, `secret`, `key`, `credential`) from all log output.

```rust
// dbench-security/src/logging.rs
pub struct SensitiveFieldFilter;

impl<S: Subscriber> Layer<S> for SensitiveFieldFilter {
    // strips sensitive fields before they reach any appender
}
```

### Stack Allocation for Small Secrets

Small secrets (passwords < 256 bytes) are stored on the stack when possible to avoid heap allocation and reduce the risk of memory scanning.

---

## Layer 4 — Application Security (Tauri)

### Content Security Policy

Tauri's WebView has a strict CSP that prevents XSS from escalating to native IPC calls:

```json
{
  "security": {
    "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost"
  }
}
```

### IPC Validation

Every Tauri command validates its inputs before processing:

```rust
#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    connection_id: Uuid,        // validated: must exist in connection registry
    query: String,              // validated: non-empty, length-bounded
    params: Vec<SqlValue>,      // validated: each value type-checked
) -> Result<QueryResult, CatalystError> {
    // ...
}
```

### Capability-Based Permissions (Tauri v2)

Tauri v2 uses a capability model. The frontend can only invoke explicitly listed commands. No wildcard permissions. Each capability is scoped to the minimum required.

---

## Layer 5 — Data Security

### Read-Only Connection Mode

Connections can be flagged as read-only. The driver enforces this at the connection level (not just UI):

- **PostgreSQL**: connects with `default_transaction_read_only = on`
- **MySQL**: uses `GRANT SELECT` connection or `SET SESSION TRANSACTION READ ONLY`
- **MongoDB**: applies `readPreference` and rejects write operations
- **Generic**: engine-level guard rejects any non-SELECT/non-read query before it reaches the driver

```rust
pub enum ConnectionMode {
    ReadWrite,
    ReadOnly,           // enforced at driver AND engine level
    ReadOnlyNoExport,   // read-only + disables CSV/JSON export
}
```

### No Query String Interpolation

The query engine never builds queries by string concatenation with user values. All parameters go through the driver's parameterized query API:

```rust
// WRONG — never do this:
let query = format!("SELECT * FROM users WHERE id = {}", user_input);

// RIGHT — always use params:
let query = Query::new("SELECT * FROM users WHERE id = $1")
    .bind(user_input);
```

### Column Masking / PII Protection

Columns can be tagged in connection config as sensitive (e.g., `ssn`, `credit_card`, `password_hash`). The engine masks these before sending results to the UI:

```rust
#[derive(Deserialize)]
pub struct ColumnMaskConfig {
    pub column_pattern: Regex,
    pub mask_mode: MaskMode,
}

pub enum MaskMode {
    FullRedact,              // "***"
    Partial { show_last: usize },  // "****1234"
    Hash,                    // SHA-256 of value (for correlation without exposure)
}
```

### Export Confirmation

Exporting query results shows a confirmation dialog listing how many rows and which columns (highlighting masked ones) will be exported. Export to disk is logged in the audit trail.

---

## Layer 6 — Audit & Compliance

### Audit Log Format

Every significant action is written to an append-only audit log in structured JSON (NDJSON):

```json
{"ts":"2026-01-15T10:23:41Z","event":"query.execute","conn_id":"pg-prod-01","db":"myapp","query_hash":"sha256:abc123","rows_returned":142,"duration_ms":23,"user":"hadihaider","host":"MacBook-Pro.local"}
{"ts":"2026-01-15T10:24:01Z","event":"connection.opened","conn_id":"pg-prod-01","db_type":"postgres","host":"db.example.com","port":5432,"tls":true,"user":"hadihaider"}
{"ts":"2026-01-15T10:25:00Z","event":"data.exported","conn_id":"pg-prod-01","format":"csv","rows":142,"destination":"/Users/hadihaider/Downloads/export.csv","user":"hadihaider"}
```

**Audit events captured:**

- `connection.opened` / `connection.closed`
- `query.execute` (query hash, not raw query, to avoid logging PII in queries)
- `query.failed`
- `data.exported`
- `schema.inspected`
- `connection.config.modified`
- `app.started` / `app.stopped`

### Tamper-Evident Log Chaining

Each log entry includes the SHA-256 hash of the previous entry, forming a chain. Any tampering with historical entries is detectable.

```json
{
  "ts": "...",
  "event": "...",
  "prev_hash": "sha256:abc123",
  "hash": "sha256:def456"
}
```

### Log Storage

- Default location: `~/.local/share/catalyst/audit/audit-YYYY-MM.jsonl`
- Monthly rotation
- Configurable retention (default: 90 days)
- Planned: forwarding to syslog, Splunk or Datadog

---

## Layer 7 — Secrets Manager Integration (planned)

> Not implemented yet. This section describes the intended design.

DBench will be able to fetch credentials from external secrets managers instead of the OS keychain:

| Provider             | Auth Method                         |
| -------------------- | ----------------------------------- |
| HashiCorp Vault      | Token, AppRole, OIDC                |
| AWS Secrets Manager  | IAM role, access key                |
| Azure Key Vault      | Managed identity, service principal |
| GCP Secret Manager   | Service account                     |
| 1Password (personal) | 1Password CLI (`op`)                |
| Bitwarden (personal) | Bitwarden CLI (`bw`)                |

Connection configs reference secrets by path:

```toml
[connections.prod-db]
driver = "postgres"
host = "db.example.com"
port = 5432
database = "myapp"
username = "appuser"

[connections.prod-db.secret]
provider = "vault"
path = "secret/data/prod/db"
field = "password"
```

Credentials are fetched at connection time, never cached longer than the connection lifetime.

---

## Security Checklist (for contributors)

Before submitting a PR that touches security-sensitive code:

- [ ] No credentials/secrets in source code, tests, or fixtures
- [ ] No `unwrap()` on Result types in security-critical paths
- [ ] Sensitive fields wrapped in `Secret<T>`
- [ ] New keychain entries follow naming convention: `catalyst/<id>/<field>`
- [ ] New Tauri commands are listed in the capability manifest
- [ ] New audit events are documented and emitted
- [ ] TLS configuration is not weakened
- [ ] Query parameters are never string-interpolated
- [ ] New dependencies reviewed for supply chain risk (`cargo audit`)

---

## Dependency Security

- `cargo audit` runs on every CI build (blocks on HIGH/CRITICAL)
- `cargo deny` enforces license policy and bans known-bad crate versions
- `cargo machete` removes unused dependencies (reduces attack surface)
- Dependabot is configured for automated security updates

## Cryptographic Primitives Used

| Purpose              | Algorithm          | Crate                |
| -------------------- | ------------------ | -------------------- |
| Symmetric encryption | AES-256-GCM        | `aes-gcm`            |
| Key derivation       | Argon2id           | `argon2`             |
| Hashing              | SHA-256, SHA-512   | `sha2`               |
| Secure random        | CSPRNG (OS-backed) | `rand` + `getrandom` |
| TLS                  | TLS 1.3 / 1.2      | `rustls`             |
| SSH                  | Ed25519, RSA-4096  | `ssh2`               |
| Memory zeroing       | —                  | `zeroize`            |
| Secret wrapping      | —                  | `secrecy`            |
