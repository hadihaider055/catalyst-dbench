//! Shared helpers for HTTP/JSON-based drivers (Elasticsearch, SurrealDB).

use std::time::Duration;

use dbench_core::result::{Column, ColumnType, Row, Value};
use dbench_security::tls::TlsConfig;
use reqwest::Client;
use serde_json::{Map, Value as Json};

/// Build an HTTP client. Certificates are always verified (webpki roots plus the
/// optional custom CA bundle); there is deliberately no "accept invalid certs" path.
pub(crate) fn build_client(tls: &TlsConfig, timeout: Duration) -> Result<Client, String> {
    let mut builder = Client::builder()
        .use_rustls_tls()
        .https_only(tls.mode.is_encrypted())
        .timeout(timeout)
        .connect_timeout(timeout);
    if let Some(path) = &tls.ca_cert_path {
        let pem = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for cert in reqwest::Certificate::from_pem_bundle(&pem).map_err(|e| e.to_string())? {
            builder = builder.add_root_certificate(cert);
        }
    }
    builder.build().map_err(|e| e.to_string())
}

/// Convert a JSON value into a grid cell.
pub(crate) fn json_to_value(v: Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(b),
        Json::Number(n) => n
            .as_i64()
            .map_or_else(|| Value::Float(n.as_f64().unwrap_or_default()), Value::Int),
        Json::String(s) => Value::Text(s),
        Json::Array(_) => Value::Json(v),
        Json::Object(o) => Value::Object(o),
    }
}

/// Best-effort column type for a type name or sample JSON value.
pub(crate) fn json_type(name: &str) -> ColumnType {
    match name {
        "int" | "integer" | "number" => ColumnType::Integer,
        "float" | "decimal" => ColumnType::Float,
        "bool" | "boolean" => ColumnType::Boolean,
        "datetime" => ColumnType::Timestamp,
        "array" => ColumnType::Array,
        "object" => ColumnType::Object,
        _ => ColumnType::Text,
    }
}

fn sample_type(v: &Json) -> ColumnType {
    match v {
        Json::Bool(_) => ColumnType::Boolean,
        Json::Number(n) if n.is_i64() => ColumnType::Integer,
        Json::Number(_) => ColumnType::Float,
        Json::Array(_) => ColumnType::Array,
        Json::Object(_) => ColumnType::Object,
        _ => ColumnType::Text,
    }
}

/// Turn a list of JSON objects into columns (union of keys, first-seen order) and rows.
pub(crate) fn rows_from_objects(objs: &[Map<String, Json>]) -> (Vec<Column>, Vec<Row>) {
    let mut names: Vec<String> = Vec::new();
    for o in objs {
        for k in o.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
    }
    let columns = names
        .iter()
        .map(|n| {
            let col_type = objs
                .iter()
                .find_map(|o| o.get(n).filter(|v| !v.is_null()))
                .map_or(ColumnType::Text, sample_type);
            Column {
                name: n.clone(),
                native_type: format!("{col_type:?}").to_lowercase(),
                col_type,
                nullable: true,
            }
        })
        .collect();
    let rows = objs
        .iter()
        .map(|o| Row {
            values: names
                .iter()
                .map(|n| o.get(n).cloned().map_or(Value::Null, json_to_value))
                .collect(),
        })
        .collect();
    (columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn objects_become_union_of_columns() {
        let objs: Vec<_> = [json!({"a": 1, "b": "x"}), json!({"b": "y", "c": true})]
            .into_iter()
            .map(|v| v.as_object().cloned().unwrap())
            .collect();
        let (cols, rows) = rows_from_objects(&objs);
        assert_eq!(
            cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(cols[0].col_type, ColumnType::Integer);
        assert!(rows[1].values[0].is_null());
    }
}
