//! Tauri IPC command modules.
//!
//! Every function here is a `#[tauri::command]` callable from the React frontend.
//! All inputs are validated before reaching the engine.

pub mod app;
pub mod connections;
pub mod query;
pub mod schema;
