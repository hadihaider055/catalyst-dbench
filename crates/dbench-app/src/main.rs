//! Catalyst DBench desktop application entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod commands;
mod state;

use std::sync::Arc;

use dashmap::DashMap;
use dbench_engine::{ConnectionRegistry, QueryExecutor};
use dbench_security::{audit::AuditLogger, logging::init_logging};
use state::AppState;
use tauri::Manager;

fn main() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            let audit_path = app
                .path()
                .app_log_dir()
                .expect("log dir unavailable")
                .join("audit.jsonl");

            let audit = AuditLogger::new(&audit_path).expect("failed to initialize audit logger");

            // Share a single registry between AppState and QueryExecutor.
            let registry = Arc::new(ConnectionRegistry::new());
            let executor = QueryExecutor::new(Arc::clone(&registry), audit.clone());

            app.manage(AppState {
                registry,
                executor,
                audit,
                ssh_tunnels: Arc::new(DashMap::new()),
                cancel_tokens: Arc::new(DashMap::new()),
            });

            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "Catalyst DBench started"
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connections::list_connections,
            commands::connections::add_connection,
            commands::connections::remove_connection,
            commands::connections::test_connection,
            commands::connections::open_connection,
            commands::connections::close_connection,
            commands::connections::store_credential,
            commands::connections::get_credential,
            commands::connections::list_databases,
            commands::query::execute_query,
            commands::query::cancel_query,
            commands::query::execute_batch,
            commands::schema::get_schema,
            commands::schema::get_object_ddl,
            commands::app::get_version,
            commands::app::get_audit_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Catalyst DBench");
}
