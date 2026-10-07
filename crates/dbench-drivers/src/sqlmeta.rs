//! Shared helper: group flat catalog rows into tables/views (SQL Server, Oracle).

use std::collections::BTreeMap;

use dbench_core::schema::{ColumnSchema, ForeignKeySchema, SchemaObject, TableSchema, ViewSchema};

/// One column as read from a catalog view.
pub(crate) struct ColRow {
    pub schema: String,
    pub table: String,
    pub is_view: bool,
    pub column: String,
    pub native_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_pk: bool,
}

/// One foreign-key column pair.
pub(crate) struct FkRow {
    pub name: String,
    pub schema: String,
    pub table: String,
    pub column: String,
    pub ref_table: String,
    pub ref_column: String,
}

/// Group rows (already ordered by schema, table, ordinal) into schema objects.
pub(crate) fn build_objects(cols: Vec<ColRow>, fks: Vec<FkRow>) -> Vec<SchemaObject> {
    let mut fk_map: BTreeMap<(String, String), BTreeMap<String, ForeignKeySchema>> =
        BTreeMap::new();
    for fk in fks {
        let entry = fk_map
            .entry((fk.schema, fk.table))
            .or_default()
            .entry(fk.name.clone())
            .or_insert_with(|| ForeignKeySchema {
                name: fk.name,
                columns: vec![],
                referenced_table: fk.ref_table,
                referenced_columns: vec![],
                on_delete: None,
                on_update: None,
            });
        entry.columns.push(fk.column);
        entry.referenced_columns.push(fk.ref_column);
    }

    let mut grouped: BTreeMap<(String, String), (bool, Vec<ColumnSchema>)> = BTreeMap::new();
    for c in cols {
        let (_, columns) = grouped
            .entry((c.schema, c.table))
            .or_insert((c.is_view, vec![]));
        columns.push(ColumnSchema {
            ordinal: columns.len() as u32 + 1,
            name: c.column,
            native_type: c.native_type,
            nullable: c.nullable,
            default_value: c.default_value,
            is_primary_key: c.is_pk,
            is_unique: c.is_pk,
            comment: None,
        });
    }

    grouped
        .into_iter()
        .map(|((schema, name), (is_view, columns))| {
            if is_view {
                SchemaObject::View(ViewSchema {
                    schema: Some(schema),
                    name,
                    columns,
                    definition: None,
                    is_materialized: false,
                })
            } else {
                let foreign_keys = fk_map
                    .remove(&(schema.clone(), name.clone()))
                    .map(|m| m.into_values().collect())
                    .unwrap_or_default();
                SchemaObject::Table(TableSchema {
                    schema: Some(schema),
                    name,
                    columns,
                    indexes: vec![],
                    foreign_keys,
                    row_count: None,
                    comment: None,
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(table: &str, column: &str, is_pk: bool) -> ColRow {
        ColRow {
            schema: "dbo".into(),
            table: table.into(),
            is_view: false,
            column: column.into(),
            native_type: "int".into(),
            nullable: !is_pk,
            default_value: None,
            is_pk,
        }
    }

    #[test]
    fn groups_columns_and_composite_fks() {
        let cols = vec![
            col("orders", "id", true),
            col("orders", "user_id", false),
            col("users", "id", true),
        ];
        let fks = vec![FkRow {
            name: "fk_user".into(),
            schema: "dbo".into(),
            table: "orders".into(),
            column: "user_id".into(),
            ref_table: "users".into(),
            ref_column: "id".into(),
        }];
        let objs = build_objects(cols, fks);
        assert_eq!(objs.len(), 2);
        let SchemaObject::Table(orders) = &objs[0] else {
            panic!("expected table")
        };
        assert_eq!(orders.columns.len(), 2);
        assert_eq!(orders.foreign_keys[0].referenced_table, "users");
    }
}
