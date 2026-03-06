//! Catalyst DBench desktop application entry point.
//!
//! Tauri v2 app that bridges the React frontend with the Rust engine.
//! All database operations are exposed as Tauri commands (type-safe IPC).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod commands;
mod state;

use dbench_engine::{ConnectionRegistry, QueryExecutor};
use dbench_security::{audit::AuditLogger, logging::init_logging};
use state::AppState;
use tauri::Manager;

fn main() {
    init_logging();

    tauri::Builder::default()
        // --- Plugins ---
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        // --- App state ---
        .setup(|app| {
            let audit_path = app
                .path()
                .app_log_dir()
                .expect("log dir unavailable")
                .join("audit.jsonl");

            let audit = AuditLogger::new(&audit_path)
                .expect("failed to initialize audit logger");

            let registry = ConnectionRegistry::new();
            let executor = QueryExecutor::new(
                ConnectionRegistry::new(), // executor owns its own registry copy
                audit.clone(),
            );

            let state = AppState {
                registry,
                executor,
                audit,
            };

            app.manage(state);

            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "Catalyst DBench started"
            );

            Ok(())
        })
        // --- IPC Commands ---
        .invoke_handler(tauri::generate_handler![
            // Connection management
            commands::connections::list_connections,
            commands::connections::add_connection,
            commands::connections::remove_connection,
            commands::connections::test_connection,
            commands::connections::open_connection,
            commands::connections::close_connection,
            // Query execution
            commands::query::execute_query,
            commands::query::execute_batch,
            // Schema inspection
            commands::schema::get_schema,
            // App / settings
            commands::app::get_version,
            commands::app::get_audit_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Catalyst DBench");
}
