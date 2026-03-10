//! Tauri commands for schema introspection.

use dbench_core::{query::Query, schema::DatabaseSchema, Value};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

/// Get the full schema for a connection (tables, views, indexes, etc.).
#[tauri::command]
pub async fn get_schema(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<DatabaseSchema, String> {
    let conn_id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;
    state
        .executor
        .inspect_schema(conn_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get the DDL (CREATE statement) for a specific object.
#[tauri::command]
pub async fn get_object_ddl(
    state: State<'_, AppState>,
    connection_id: String,
    db_type: String,
    object_name: String,
    schema_name: Option<String>,
    kind: String,
) -> Result<String, String> {
    let conn_id = Uuid::parse_str(&connection_id).map_err(|e| e.to_string())?;

    let fqn = match &schema_name {
        Some(s) if !s.is_empty() => format!("{}.{}", s, object_name),
        _ => object_name.clone(),
    };

    let ddl_sql: String = match db_type.as_str() {
        "mysql" => format!("SHOW CREATE TABLE `{}`;", fqn),
        "clickhouse" => format!("SHOW CREATE TABLE {};", fqn),
        "sqlite" => format!(
            "SELECT sql FROM sqlite_master WHERE (type='table' OR type='view') AND name='{}';",
            object_name
        ),
        "cassandra" => {
            // Construct DDL from system_schema
            let ks = schema_name.as_deref().unwrap_or("system");
            format!(
                "SELECT column_name, type, kind FROM system_schema.columns \
                 WHERE keyspace_name='{}' AND table_name='{}';",
                ks, object_name
            )
        }
        "postgres" | "cockroachdb" => {
            let schema = schema_name.as_deref().unwrap_or("public");
            let obj_kind = if kind == "view" { "view" } else { "table" };
            if obj_kind == "view" {
                format!(
                    "SELECT 'CREATE OR REPLACE VIEW {fqn} AS' || chr(10) || view_definition \
                     FROM information_schema.views \
                     WHERE table_schema='{}' AND table_name='{}';",
                    schema, object_name
                )
            } else {
                // Build CREATE TABLE from information_schema columns
                format!(
                    "SELECT 'CREATE TABLE {fqn} (' || chr(10) || \
                     string_agg('  ' || column_name || ' ' || udt_name || \
                       CASE WHEN character_maximum_length IS NOT NULL \
                            THEN '(' || character_maximum_length || ')' ELSE '' END || \
                       CASE WHEN is_nullable='NO' THEN ' NOT NULL' ELSE '' END, \
                       ',' || chr(10) ORDER BY ordinal_position) || \
                     chr(10) || ');' \
                     FROM information_schema.columns \
                     WHERE table_schema='{}' AND table_name='{}' \
                     GROUP BY table_name;",
                    schema, object_name
                )
            }
        }
        _ => {
            return Err(format!("DDL introspection not supported for {}", db_type));
        }
    };

    let result = state
        .executor
        .execute(conn_id, Query::new(ddl_sql))
        .await
        .map_err(|e| e.to_string())?;

    // For Cassandra, synthesize DDL from column rows
    if db_type == "cassandra" {
        let ks = schema_name.as_deref().unwrap_or("system");
        let mut parts: Vec<String> = vec![format!(
            "CREATE TABLE {}.{} (",
            ks, object_name
        )];
        let col_idx = result.columns.iter().position(|c| c.name == "column_name").unwrap_or(0);
        let type_idx = result.columns.iter().position(|c| c.name == "type").unwrap_or(1);
        let kind_idx = result.columns.iter().position(|c| c.name == "kind").unwrap_or(2);
        let mut cols: Vec<String> = Vec::new();
        let mut pk_cols: Vec<String> = Vec::new();
        for row in &result.rows {
            let col_name = value_as_str(row.values.get(col_idx));
            let col_type = value_as_str(row.values.get(type_idx));
            let col_kind = value_as_str(row.values.get(kind_idx));
            if col_kind == "partition_key" || col_kind == "clustering" {
                pk_cols.push(col_name.clone());
            }
            cols.push(format!("  {} {}", col_name, col_type));
        }
        if !pk_cols.is_empty() {
            cols.push(format!("  PRIMARY KEY ({})", pk_cols.join(", ")));
        }
        parts.push(cols.join(",\n"));
        parts.push(");".to_string());
        return Ok(parts.join("\n"));
    }

    // MySQL SHOW CREATE TABLE returns 2 columns; DDL is in column index 1
    let ddl_col = if db_type == "mysql" { 1 } else { 0 };
    let ddl = result
        .rows
        .first()
        .and_then(|r| r.values.get(ddl_col))
        .map(|v| value_as_str(Some(v)))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("No DDL found for {}", fqn))?;

    Ok(ddl)
}

fn value_as_str(v: Option<&Value>) -> String {
    match v {
        Some(Value::Text(s)) => s.clone(),
        Some(Value::Int(n)) => n.to_string(),
        _ => String::new(),
    }
}
