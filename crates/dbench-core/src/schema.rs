//! Schema introspection types.
//!
//! A unified representation of database structure across all supported databases.
//! SQL tables, MongoDB collections, Redis key patterns, etc. all map to these types.

use serde::{Deserialize, Serialize};

use crate::types::DatabaseType;

/// The full inspected schema of a database/instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    /// Database or instance name.
    pub name: String,
    /// Database type.
    pub db_type: DatabaseType,
    /// Server version string.
    pub server_version: String,
    /// All schema objects (tables, collections, views, etc.).
    pub objects: Vec<SchemaObject>,
}

/// A top-level schema object (table, view, collection, stream, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaObject {
    /// A relational table.
    Table(TableSchema),
    /// A database view.
    View(ViewSchema),
    /// A MongoDB-style document collection.
    Collection(CollectionSchema),
    /// An index (standalone, e.g., in Elasticsearch).
    Index(IndexSchema),
    /// A Redis key pattern group.
    KeyPattern(KeyPatternSchema),
    /// A stored procedure or function.
    Procedure(ProcedureSchema),
}

impl SchemaObject {
    /// The display name of this object.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Table(t) => &t.name,
            Self::View(v) => &v.name,
            Self::Collection(c) => &c.name,
            Self::Index(i) => &i.name,
            Self::KeyPattern(k) => &k.pattern,
            Self::Procedure(p) => &p.name,
        }
    }
}

/// Schema for a relational table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// Schema/namespace (e.g., `"public"` in Postgres).
    pub schema: Option<String>,
    /// Table name.
    pub name: String,
    /// Column definitions.
    pub columns: Vec<ColumnSchema>,
    /// Table indexes.
    pub indexes: Vec<IndexSchema>,
    /// Foreign key relationships.
    pub foreign_keys: Vec<ForeignKeySchema>,
    /// Approximate row count (may be estimated).
    pub row_count: Option<u64>,
    /// Table comment/description.
    pub comment: Option<String>,
}

/// Schema for a database view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSchema {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnSchema>,
    pub definition: Option<String>,
    pub is_materialized: bool,
}

/// Schema for a document collection (MongoDB, CouchDB, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSchema {
    /// The database this collection belongs to (used when listing across multiple databases).
    pub database: Option<String>,
    pub name: String,
    /// Inferred field schema from a sample of documents.
    pub inferred_fields: Vec<InferredField>,
    pub indexes: Vec<IndexSchema>,
    pub document_count: Option<u64>,
    pub size_bytes: Option<u64>,
}

/// A column in a relational table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub ordinal: u32,
    pub native_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub is_unique: bool,
    pub comment: Option<String>,
}

/// An index on a table or collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSchema {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: String, // "btree", "hash", "gin", etc.
}

/// A foreign key relationship between two tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeySchema {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

/// An inferred field from sampling a document collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredField {
    pub path: String,      // e.g., "address.city"
    pub inferred_type: String,
    pub occurrence_rate: f64, // 0.0–1.0, what fraction of sampled docs have this field
}

/// A Redis key pattern group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPatternSchema {
    pub pattern: String,
    pub key_type: String, // "string", "list", "hash", "set", "zset", "stream"
    pub sample_count: u64,
}

/// A stored procedure or function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureSchema {
    pub schema: Option<String>,
    pub name: String,
    pub language: Option<String>,
    pub definition: Option<String>,
}
