//! Query types — parameterized, injection-safe database queries.
//!
//! Catalyst DBench **never** builds queries by string interpolation.
//! All user values must be passed as [`QueryParam`] bindings.

use serde::{Deserialize, Serialize};

use crate::result::Value;

/// A parameterized database query.
///
/// # Example
///
/// ```rust
/// use dbench_core::Query;
///
/// let query = Query::new("SELECT * FROM users WHERE id = $1 AND active = $2")
///     .bind(42_i64)
///     .bind(true);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// The query text with parameter placeholders.
    /// Placeholder syntax varies by driver (`$1` for Postgres, `?` for MySQL, etc.)
    /// — drivers are responsible for adapting the syntax.
    pub text: String,
    /// Bound parameter values, in order.
    pub params: Vec<QueryParam>,
    /// Optional query timeout in milliseconds. `None` uses the driver default.
    pub timeout_ms: Option<u64>,
    /// If `true`, the driver should only plan/explain the query, not execute it.
    pub explain: bool,
}

impl Query {
    /// Create a new query with the given text and no parameters.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            params: Vec::new(),
            timeout_ms: None,
            explain: false,
        }
    }

    /// Bind a parameter value to this query.
    ///
    /// Parameters are positional. Each call appends to the parameter list.
    #[must_use]
    pub fn bind(mut self, value: impl Into<QueryParam>) -> Self {
        self.params.push(value.into());
        self
    }

    /// Set a per-query timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Set this as an EXPLAIN/ANALYZE query (no actual execution).
    #[must_use]
    pub fn explain(mut self) -> Self {
        self.explain = true;
        self
    }

    /// Returns `true` if this query has no parameters.
    #[must_use]
    pub fn is_raw(&self) -> bool {
        self.params.is_empty()
    }
}

/// A single bound parameter value.
///
/// This enum covers all value types that Catalyst DBench drivers are expected to support.
/// Drivers map these to their native types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum QueryParam {
    /// SQL NULL / JSON null
    Null,
    /// Boolean
    Bool(bool),
    /// 64-bit integer
    Int(i64),
    /// 64-bit float
    Float(f64),
    /// UTF-8 string
    Text(String),
    /// Raw bytes (BYTEA, BLOB)
    Bytes(Vec<u8>),
    /// JSON value (for document DBs, JSON columns)
    Json(serde_json::Value),
    /// UUID
    Uuid(uuid::Uuid),
    /// ISO-8601 timestamp string (drivers parse to native datetime)
    Timestamp(String),
}

// Convenient From impls for common types.
impl From<bool> for QueryParam {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i32> for QueryParam {
    fn from(v: i32) -> Self {
        Self::Int(v.into())
    }
}
impl From<i64> for QueryParam {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<f64> for QueryParam {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}
impl From<String> for QueryParam {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<&str> for QueryParam {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}
impl From<uuid::Uuid> for QueryParam {
    fn from(v: uuid::Uuid) -> Self {
        Self::Uuid(v)
    }
}
impl From<serde_json::Value> for QueryParam {
    fn from(v: serde_json::Value) -> Self {
        Self::Json(v)
    }
}
impl<T: Into<QueryParam>> From<Option<T>> for QueryParam {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(inner) => inner.into(),
            None => Self::Null,
        }
    }
}
