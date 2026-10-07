//! Query result types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The result of executing a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Column metadata, in order.
    pub columns: Vec<Column>,
    /// Data rows.
    pub rows: Vec<Row>,
    /// Number of rows affected (for INSERT/UPDATE/DELETE).
    pub rows_affected: Option<u64>,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Query plan (populated only when `Query::explain` is set).
    pub explain_plan: Option<String>,
}

impl QueryResult {
    /// Total number of rows returned.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Whether this result has any data rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Metadata for a single column in a result set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// Column name as returned by the database.
    pub name: String,
    /// The Catalyst DBench-normalized type.
    pub col_type: ColumnType,
    /// Whether this column can contain NULL values.
    pub nullable: bool,
    /// Database-native type string (e.g., `"varchar(255)"`, `"timestamptz"`).
    pub native_type: String,
}

/// Catalyst DBench's normalized column type system.
///
/// Maps database-specific types to a consistent set of variants.
/// The `native_type` field on [`Column`] always preserves the original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    /// Any integer type (TINYINT through BIGINT).
    Integer,
    /// Floating-point type (FLOAT, DOUBLE, REAL).
    Float,
    /// Fixed-precision decimal (NUMERIC, DECIMAL).
    Decimal,
    /// Boolean.
    Boolean,
    /// Text / string (VARCHAR, TEXT, CHAR, etc.).
    Text,
    /// Raw binary data (BYTEA, BLOB, BINARY).
    Bytes,
    /// Date (no time component).
    Date,
    /// Time (no date component).
    Time,
    /// Full timestamp (date + time, with or without timezone).
    Timestamp,
    /// JSON / JSONB / document.
    Json,
    /// UUID.
    Uuid,
    /// Array of another type.
    Array,
    /// Object / embedded document (for document databases).
    Object,
    /// A type not covered by the above (preserved as native).
    Unknown,
}

/// A single data row: an ordered list of values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub values: Vec<Value>,
}

impl Row {
    /// Get a value by column index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.values.get(index)
    }
}

/// A single cell value in a query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "v", rename_all = "snake_case")]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Decimal as string to preserve exact precision.
    Decimal(String),
    Text(String),
    Bytes(Vec<u8>),
    Date(String), // ISO-8601 date string
    Time(String), // ISO-8601 time string
    Timestamp(DateTime<Utc>),
    Json(serde_json::Value),
    Uuid(Uuid),
    Array(Vec<Value>),
    Object(serde_json::Map<String, serde_json::Value>),
}

impl Value {
    /// Returns `true` if this value is NULL.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns a display string for the value (for the data grid).
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Null => "NULL".into(),
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Decimal(s) | Self::Text(s) | Self::Date(s) | Self::Time(s) => s.clone(),
            Self::Bytes(b) => format!("\\x{}", hex::encode(b)),
            Self::Timestamp(t) => t.to_rfc3339(),
            Self::Json(j) => j.to_string(),
            Self::Uuid(u) => u.to_string(),
            Self::Array(a) => format!(
                "[{}]",
                a.iter().map(Self::display).collect::<Vec<_>>().join(", ")
            ),
            Self::Object(o) => serde_json::to_string(o).unwrap_or_else(|_| "{}".into()),
        }
    }
}
