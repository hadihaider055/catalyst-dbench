// Typed wrappers around every Tauri IPC command.
// All database I/O goes through here — never call invoke() directly in components.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  ConnectionInfo,
  ConnectionPayload,
  DatabaseSchema,
  QueryPayload,
  QueryResult,
} from "./types";
import { withUriPassword } from "./utils";

/** Tauri's IPC bridge only exists inside the desktop window, not a browser tab. */
function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return Promise.reject(
      new Error(
        "Not running inside the desktop app. Start it with `cargo tauri dev` (in crates/dbench-app) " +
          "and use that window — localhost:5173 in a browser can't reach the database backend.",
      ),
    );
  }
  return tauriInvoke<T>(cmd, args);
}

// ── Connections ──────────────────────────────────────────────────────────────

export const listConnections = (): Promise<ConnectionInfo[]> =>
  invoke("list_connections");

export const addConnection = (
  payload: ConnectionPayload
): Promise<{ connection_id: string; info: ConnectionInfo }> =>
  invoke("add_connection", { payload: { ...payload, host: withUriPassword(payload.host, payload.password) } });

export const testConnection = (
  payload: ConnectionPayload
): Promise<string> =>
  invoke("test_connection", { payload: { ...payload, host: withUriPassword(payload.host, payload.password) } });

export const openConnection = (
  payload: ConnectionPayload
): Promise<{ connection_id: string; info: ConnectionInfo }> =>
  invoke("open_connection", { payload });

export const closeConnection = (connectionId: string): Promise<void> =>
  invoke("close_connection", { connectionId });

export const removeConnection = (connectionId: string): Promise<void> =>
  invoke("remove_connection", { connectionId });

export const storeCredential = (connectionId: string, password: string): Promise<void> =>
  invoke("store_credential", { connectionId, password });

export const getCredential = (connectionId: string): Promise<string | null> =>
  invoke("get_credential", { connectionId });

// ── Query ────────────────────────────────────────────────────────────────────

export const executeQuery = (payload: QueryPayload): Promise<QueryResult> =>
  invoke("execute_query", { payload });

export const cancelQuery = (connectionId: string): Promise<void> =>
  invoke("cancel_query", { connectionId });

export const executeBatch = (
  connectionId: string,
  queries: string[]
): Promise<QueryResult[]> =>
  invoke("execute_batch", { connectionId, queries });

// ── Schema ───────────────────────────────────────────────────────────────────

export const getSchema = (connectionId: string): Promise<DatabaseSchema> =>
  invoke("get_schema", { connectionId });

export const listDatabases = (connectionId: string): Promise<string[]> =>
  invoke("list_databases", { connectionId });

export const getObjectDdl = (
  connectionId: string,
  dbType: string,
  objectName: string,
  schemaName: string | null,
  kind: string,
): Promise<string> =>
  invoke("get_object_ddl", { connectionId, dbType, objectName, schemaName, kind });

// ── App ──────────────────────────────────────────────────────────────────────

export const getVersion = (): Promise<string> => invoke("get_version");

export const getAuditLogPath = (): Promise<string> =>
  invoke("get_audit_log_path");
