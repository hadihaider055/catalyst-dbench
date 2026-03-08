// Typed wrappers around every Tauri IPC command.
// All database I/O goes through here — never call invoke() directly in components.

import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionInfo,
  ConnectionPayload,
  DatabaseSchema,
  QueryPayload,
  QueryResult,
} from "./types";

// ── Connections ──────────────────────────────────────────────────────────────

export const listConnections = (): Promise<ConnectionInfo[]> =>
  invoke("list_connections");

export const addConnection = (
  payload: ConnectionPayload
): Promise<{ connection_id: string; info: ConnectionInfo }> =>
  invoke("add_connection", { payload });

export const testConnection = (
  payload: ConnectionPayload
): Promise<string> => invoke("test_connection", { payload });

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

// ── App ──────────────────────────────────────────────────────────────────────

export const getVersion = (): Promise<string> => invoke("get_version");

export const getAuditLogPath = (): Promise<string> =>
  invoke("get_audit_log_path");
