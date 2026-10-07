//! Tamper-evident, append-only audit logging.
//!
//! Every significant action (query execution, connection open/close, data export,
//! config changes) is recorded in a structured NDJSON log file. Each entry
//! includes the SHA-256 hash of the previous entry, forming a verifiable chain.
//!
//! Log format (NDJSON, one JSON object per line):
//! ```json
//! {"ts":"2026-01-15T10:23:41Z","event":"query.execute","conn_id":"pg-prod","query_hash":"sha256:abc","rows":142,"duration_ms":23,"prev_hash":"sha256:000","hash":"sha256:abc123"}
//! ```

use std::{io::Write, path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{crypto::sha256_hex, Result, SecurityError};

/// All event types recorded in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum AuditEvent {
    /// A new connection was successfully opened.
    ConnectionOpened {
        conn_id: String,
        db_type: String,
        host: String,
        port: u16,
        database: String,
        tls: bool,
        ssh_tunnel: bool,
    },
    /// A connection was closed.
    ConnectionClosed {
        conn_id: String,
        reason: CloseReason,
    },
    /// A query was executed successfully.
    QueryExecute {
        conn_id: String,
        /// SHA-256 of the raw query text — we hash so PII in queries is not stored.
        query_hash: String,
        rows_returned: Option<u64>,
        rows_affected: Option<u64>,
        duration_ms: u64,
    },
    /// A query failed.
    QueryFailed {
        conn_id: String,
        query_hash: String,
        error: String,
        duration_ms: u64,
    },
    /// Query results were exported to disk.
    DataExported {
        conn_id: String,
        format: ExportFormat,
        rows: u64,
        destination: String, // file path
        masked_columns: Vec<String>,
    },
    /// Schema was inspected (tables/collections listed).
    SchemaInspected { conn_id: String, object_count: u64 },
    /// A connection configuration was added, modified, or removed.
    ConnectionConfigChanged {
        conn_id: String,
        change_type: ConfigChangeType,
    },
    /// The application started.
    AppStarted { version: String, platform: String },
    /// The application stopped cleanly.
    AppStopped,
}

/// Reason a connection was closed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    UserInitiated,
    Error,
    Timeout,
    AppShutdown,
}

/// Export format for `DataExported` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
    Parquet,
}

/// Type of connection config change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigChangeType {
    Created,
    Updated,
    Deleted,
}

/// A single entry in the audit log.
#[derive(Debug, Serialize, Deserialize)]
struct AuditEntry {
    /// ISO-8601 UTC timestamp.
    ts: DateTime<Utc>,
    /// Unique ID for this log entry.
    id: Uuid,
    /// The event data.
    #[serde(flatten)]
    event: AuditEvent,
    /// System username of the operator.
    user: String,
    /// Hostname of the machine. Not named `host`: that would collide with the
    /// flattened `ConnectionOpened.host` and emit duplicate JSON keys.
    machine: String,
    /// SHA-256 of the previous entry (hex). Enables tamper detection.
    prev_hash: String,
    /// SHA-256 of this entry's content (excluding this field).
    hash: String,
}

/// Thread-safe, append-only audit logger.
///
/// Holds a mutex over the file handle to ensure atomicity of each write.
/// Each call to [`AuditLogger::log`] is an `async` operation that acquires
/// the lock, writes, and flushes before releasing.
#[derive(Clone)]
pub struct AuditLogger {
    inner: Arc<Mutex<AuditLoggerInner>>,
}

struct AuditLoggerInner {
    file: std::fs::File,
    last_hash: String,
    user: String,
    host: String,
}

impl AuditLogger {
    /// Create or open an audit log at the given path.
    ///
    /// If the file exists, the last entry's hash is read to continue the chain.
    ///
    /// # Errors
    /// Fails if the file cannot be opened or the existing log is unreadable.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        // Read the last hash from the existing log (if any) to continue the chain.
        let last_hash = Self::read_last_hash(&path).unwrap_or_else(|| "genesis".to_owned());

        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_owned());

        let host = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_owned());

        Ok(Self {
            inner: Arc::new(Mutex::new(AuditLoggerInner {
                file,
                last_hash,
                user,
                host,
            })),
        })
    }

    /// Append an audit event to the log.
    ///
    /// This method is async to allow calling from async Tauri commands without
    /// blocking. Internally it uses a blocking file write inside `spawn_blocking`.
    ///
    /// # Errors
    /// Fails if the log file cannot be written.
    pub async fn log(&self, event: AuditEvent) -> Result<()> {
        let mut inner = self.inner.lock().await;
        inner.write_entry(event)
    }

    fn read_last_hash(path: &Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        let last_line = content.lines().rev().find(|l| !l.trim().is_empty())?;
        let entry: serde_json::Value = serde_json::from_str(last_line).ok()?;
        entry["hash"].as_str().map(str::to_owned)
    }
}

impl AuditLoggerInner {
    fn write_entry(&mut self, event: AuditEvent) -> Result<()> {
        let id = Uuid::new_v4();
        let ts = Utc::now();

        // Compute content hash (event + metadata, excluding the `hash` field itself).
        let content = serde_json::json!({
            "ts": ts,
            "id": id,
            "event": &event,
            "user": &self.user,
            "machine": &self.host,
            "prev_hash": &self.last_hash,
        });
        let content_str =
            serde_json::to_string(&content).map_err(|e| SecurityError::Audit(e.to_string()))?;
        let hash = format!("sha256:{}", sha256_hex(content_str.as_bytes()));

        let entry = AuditEntry {
            ts,
            id,
            event,
            user: self.user.clone(),
            machine: self.host.clone(),
            prev_hash: self.last_hash.clone(),
            hash: hash.clone(),
        };

        let line =
            serde_json::to_string(&entry).map_err(|e| SecurityError::Audit(e.to_string()))?;

        writeln!(self.file, "{line}").map_err(|e| SecurityError::Audit(e.to_string()))?;
        self.file
            .flush()
            .map_err(|e| SecurityError::Audit(e.to_string()))?;

        self.last_hash = hash;
        tracing::debug!(event = ?entry.event, "Audit event written");
        Ok(())
    }
}

/// Verify the integrity of an audit log by re-computing and checking each hash.
///
/// Returns `Ok(entry_count)` if the chain is intact, or `Err` with the
/// index of the first broken entry.
///
/// # Errors
/// Returns an error message indicating which entry was tampered.
pub fn verify_audit_log(path: impl AsRef<Path>) -> std::result::Result<usize, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut prev_hash = "genesis".to_owned();
    let mut count = 0;

    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let entry: serde_json::Value =
            serde_json::from_str(line).map_err(|_| format!("Entry {i}: invalid JSON"))?;

        let stored_prev = entry["prev_hash"]
            .as_str()
            .ok_or_else(|| format!("Entry {i}: missing prev_hash"))?;

        if stored_prev != prev_hash {
            return Err(format!(
                "Entry {i}: chain broken — expected prev_hash '{prev_hash}', got '{stored_prev}'"
            ));
        }

        let stored_hash = entry["hash"]
            .as_str()
            .ok_or_else(|| format!("Entry {i}: missing hash"))?
            .to_owned();

        // Re-derive the hashed content exactly as `write_entry` built it: the event is
        // flattened into the entry, so every non-metadata key belongs to the event.
        let obj = entry
            .as_object()
            .ok_or_else(|| format!("Entry {i}: not an object"))?;
        let event: serde_json::Map<String, serde_json::Value> = obj
            .iter()
            .filter(|(k, _)| {
                !matches!(
                    k.as_str(),
                    "ts" | "id" | "user" | "machine" | "prev_hash" | "hash"
                )
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let content = serde_json::json!({
            "ts": entry["ts"],
            "id": entry["id"],
            "event": event,
            "user": entry["user"],
            "machine": entry["machine"],
            "prev_hash": stored_prev,
        });
        let content_str = serde_json::to_string(&content).map_err(|e| e.to_string())?;
        if format!("sha256:{}", sha256_hex(content_str.as_bytes())) != stored_hash {
            return Err(format!(
                "Entry {i}: content hash mismatch — entry was modified"
            ));
        }

        prev_hash = stored_hash;

        count += 1;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn audit_log_roundtrip() {
        let tmp = NamedTempFile::new().unwrap();
        let logger = AuditLogger::new(tmp.path()).unwrap();

        logger
            .log(AuditEvent::AppStarted {
                version: "0.1.0".into(),
                platform: "test".into(),
            })
            .await
            .unwrap();

        logger
            .log(AuditEvent::ConnectionOpened {
                conn_id: "test-conn".into(),
                db_type: "postgres".into(),
                host: "localhost".into(),
                port: 5432,
                database: "testdb".into(),
                tls: true,
                ssh_tunnel: false,
            })
            .await
            .unwrap();

        logger.log(AuditEvent::AppStopped).await.unwrap();

        let count = verify_audit_log(tmp.path()).unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn tampered_log_detected() {
        let tmp = NamedTempFile::new().unwrap();
        let logger = AuditLogger::new(tmp.path()).unwrap();

        logger.log(AuditEvent::AppStopped).await.unwrap();

        // Tamper: change the event but keep the hash chain fields intact.
        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let tampered = content.replace("app_stopped", "app_started");
        assert_ne!(content, tampered, "tamper must actually change the log");
        std::fs::write(tmp.path(), tampered).unwrap();

        assert!(verify_audit_log(tmp.path()).is_err());
    }
}
