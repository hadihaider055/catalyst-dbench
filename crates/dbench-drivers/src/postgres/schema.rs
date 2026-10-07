//! PostgreSQL schema introspection via `information_schema` and `pg_catalog`.

use dbench_core::{
    error::CatalystError,
    schema::{
        ColumnSchema, DatabaseSchema, ForeignKeySchema, SchemaObject, TableSchema, ViewSchema,
    },
    types::DatabaseType,
    Result,
};

pub async fn inspect(client: &tokio_postgres::Client, db_name: &str) -> Result<DatabaseSchema> {
    tracing::debug!("Inspecting PostgreSQL schema");

    let server_version = client
        .query_one("SELECT version()", &[])
        .await
        .ok()
        .and_then(|r| r.try_get::<_, String>(0).ok())
        .map(|v| v.split(' ').nth(1).unwrap_or("unknown").to_string())
        .unwrap_or_else(|| "unknown".into());

    // Fetch all tables.
    let table_rows = client
        .query(
            "
        SELECT t.table_schema, t.table_name,
               obj_description(c.oid, 'pg_class') AS comment
        FROM information_schema.tables t
        JOIN pg_class c ON c.relname = t.table_name
        JOIN pg_namespace n ON n.nspname = t.table_schema AND c.relnamespace = n.oid
        WHERE t.table_type = 'BASE TABLE'
          AND t.table_schema NOT IN ('pg_catalog', 'information_schema')
        ORDER BY t.table_schema, t.table_name
    ",
            &[],
        )
        .await
        .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

    // Fetch all columns.
    let col_rows = client
        .query(
            "
        SELECT c.table_schema, c.table_name, c.column_name,
               c.ordinal_position::int4, c.udt_name, c.is_nullable,
               c.column_default,
               EXISTS (
                   SELECT 1 FROM information_schema.key_column_usage kcu
                   JOIN information_schema.table_constraints tc
                     ON kcu.constraint_name = tc.constraint_name
                    AND tc.constraint_type = 'PRIMARY KEY'
                    AND tc.table_name = c.table_name
                    AND tc.table_schema = c.table_schema
                   WHERE kcu.column_name = c.column_name
               ) AS is_pk
        FROM information_schema.columns c
        WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema')
        ORDER BY c.table_schema, c.table_name, c.ordinal_position
    ",
            &[],
        )
        .await
        .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

    // Build column map: (schema, table) -> Vec<ColumnSchema>
    let mut col_map: std::collections::HashMap<(String, String), Vec<ColumnSchema>> =
        std::collections::HashMap::new();
    for row in &col_rows {
        let schema: String = row.try_get(0).unwrap_or_default();
        let table: String = row.try_get(1).unwrap_or_default();
        let col_name: String = row.try_get(2).unwrap_or_default();
        let ordinal: i32 = row.try_get(3).unwrap_or(0);
        let udt: String = row.try_get(4).unwrap_or_default();
        let nullable: String = row.try_get(5).unwrap_or_else(|_| "YES".to_string());
        let default: Option<String> = row.try_get(6).ok().flatten();
        let is_pk: bool = row.try_get(7).unwrap_or(false);

        col_map
            .entry((schema, table))
            .or_default()
            .push(ColumnSchema {
                name: col_name,
                ordinal: ordinal as u32,
                native_type: udt,
                nullable: nullable == "YES",
                default_value: default,
                is_primary_key: is_pk,
                is_unique: false,
                comment: None,
            });
    }

    // Fetch all foreign keys in a single query.
    let fk_rows = client
        .query(
            "
        SELECT
            tc.table_schema,
            tc.table_name,
            tc.constraint_name,
            kcu.column_name,
            ccu.table_name  AS referenced_table,
            ccu.column_name AS referenced_column,
            rc.delete_rule,
            rc.update_rule
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON  tc.constraint_name = kcu.constraint_name
            AND tc.table_schema    = kcu.table_schema
            AND tc.table_name      = kcu.table_name
        JOIN information_schema.constraint_column_usage ccu
            ON  ccu.constraint_name = tc.constraint_name
            AND ccu.table_schema    = tc.table_schema
        JOIN information_schema.referential_constraints rc
            ON  tc.constraint_name  = rc.constraint_name
            AND tc.table_schema     = rc.constraint_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_schema NOT IN ('pg_catalog', 'information_schema')
        ORDER BY tc.table_schema, tc.table_name, tc.constraint_name, kcu.ordinal_position
    ",
            &[],
        )
        .await
        .unwrap_or_default();

    // Build FK map: (schema, table) -> Vec<ForeignKeySchema>
    // Each constraint may span multiple columns, so we group by constraint name.
    let mut fk_map: std::collections::HashMap<
        (String, String),
        std::collections::HashMap<String, ForeignKeySchema>,
    > = std::collections::HashMap::new();

    for row in &fk_rows {
        let schema: String = row.try_get(0).unwrap_or_default();
        let table: String = row.try_get(1).unwrap_or_default();
        let cname: String = row.try_get(2).unwrap_or_default();
        let col: String = row.try_get(3).unwrap_or_default();
        let ref_tbl: String = row.try_get(4).unwrap_or_default();
        let ref_col: String = row.try_get(5).unwrap_or_default();
        let on_del: Option<String> = row.try_get(6).ok();
        let on_upd: Option<String> = row.try_get(7).ok();

        let entry = fk_map
            .entry((schema, table))
            .or_default()
            .entry(cname.clone())
            .or_insert_with(|| ForeignKeySchema {
                name: cname,
                columns: vec![],
                referenced_table: ref_tbl,
                referenced_columns: vec![],
                on_delete: on_del,
                on_update: on_upd,
            });
        entry.columns.push(col);
        entry.referenced_columns.push(ref_col);
    }

    // Build objects.
    let mut objects = Vec::new();
    for row in &table_rows {
        let schema: String = row.try_get(0).unwrap_or_default();
        let name: String = row.try_get(1).unwrap_or_default();
        let comment: Option<String> = row.try_get(2).ok().flatten();

        let columns = col_map
            .remove(&(schema.clone(), name.clone()))
            .unwrap_or_default();
        let foreign_keys = fk_map
            .remove(&(schema.clone(), name.clone()))
            .map(|m| m.into_values().collect())
            .unwrap_or_default();

        objects.push(SchemaObject::Table(TableSchema {
            schema: Some(schema),
            name,
            columns,
            indexes: vec![],
            foreign_keys,
            row_count: None,
            comment,
        }));
    }

    // Fetch views too.
    let view_rows = client
        .query(
            "
        SELECT table_schema, table_name
        FROM information_schema.views
        WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
        ORDER BY table_schema, table_name
    ",
            &[],
        )
        .await
        .map_err(|e| CatalystError::SchemaError(e.to_string()))?;

    for row in &view_rows {
        let schema: String = row.try_get(0).unwrap_or_default();
        let name: String = row.try_get(1).unwrap_or_default();
        objects.push(SchemaObject::View(ViewSchema {
            schema: Some(schema),
            name,
            columns: vec![],
            definition: None,
            is_materialized: false,
        }));
    }

    Ok(DatabaseSchema {
        name: db_name.to_string(),
        db_type: DatabaseType::Postgres,
        server_version,
        objects,
    })
}
