//! Tauri commands for application-level operations.

use tauri::Manager;

/// Get the current application version.
#[tauri::command]
pub fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Get the path to the current audit log file.
#[tauri::command]
pub async fn get_audit_log_path(app: tauri::AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .app_log_dir()
        .map_err(|e| e.to_string())?
        .join("audit.jsonl");
    Ok(path.to_string_lossy().into_owned())
}
