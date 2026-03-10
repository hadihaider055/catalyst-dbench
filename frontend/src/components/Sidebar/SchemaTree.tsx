import { useState, useEffect } from "react";
import {
  Table2,
  Eye,
  Hash,
  Trash2,
  AlertCircle,
  RefreshCw,
  Database,
  ChevronRight,
  Network,
  Code2,
} from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { executeQuery, listDatabases, getObjectDdl } from "@/lib/commands";
import type { DatabaseSchema } from "@/lib/types";
import ContextMenu, { type ContextMenuEntry } from "../ContextMenu/index";
import DangerConfirm from "./DangerConfirm";
import SchemaObjectRow from "./SchemaObjectRow";
import { cn } from "@/lib/utils";

function MongoDbGroup({
  dbName,
  colls,
  onCollClick,
  onCtxMenu,
}: {
  dbName: string;
  colls: DatabaseSchema["objects"];
  onCollClick: (name: string, database: string) => void;
  onCtxMenu: (e: React.MouseEvent, obj: DatabaseSchema["objects"][number], kind: string) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div>
      <button
        className="flex items-center gap-1 w-full px-2 py-0.5 text-2xs text-text-muted hover:text-text transition-colors"
        onClick={() => setOpen((o) => !o)}
      >
        <ChevronRight size={9} className={cn("transition-transform", open && "rotate-90")} />
        <Database size={9} />
        <span className="truncate">{dbName}</span>
      </button>
      {open &&
        colls.map((obj) => (
          <div key={`c-${obj.name as string}`} className="pl-3">
            <SchemaObjectRow
              icon={<Table2 size={10} />}
              obj={obj}
              onClick={() => onCollClick(obj.name as string, dbName)}
              onContextMenu={(e) => onCtxMenu(e, obj, "collection")}
            />
          </div>
        ))}
    </div>
  );
}

interface CtxMenu {
  x: number;
  y: number;
  name: string;
  kind: string;
  schema?: string;
}

interface Props {
  connectionId: string;
}

// DB types that support switching between databases
const MULTI_DB_TYPES = new Set(["postgres", "mysql", "cockroachdb", "clickhouse", "cassandra"]);

export default function SchemaTree({ connectionId }: Props) {
  const { schemas, loadSchema, openTab, openERDiagramTab, openDdlTab, activeConnections, switchDatabase } =
    useAppStore();
  const schema = schemas[connectionId];
  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);
  const [confirmModal, setConfirmModal] = useState<{
    sql: string;
    title: string;
  } | null>(null);
  const [databases, setDatabases] = useState<string[]>([]);
  const [switchingDb, setSwitchingDb] = useState(false);

  const conn = activeConnections.find((c) => c.id === connectionId);
  const dbType = conn?.db_type ?? "postgres";

  useEffect(() => {
    if (!conn || !MULTI_DB_TYPES.has(dbType)) return;
    listDatabases(connectionId).then(setDatabases).catch(() => setDatabases([]));
  }, [connectionId, dbType, conn]);

  const handleSwitchDatabase = async (db: string) => {
    if (db === conn?.database || switchingDb) return;
    setSwitchingDb(true);
    try {
      await switchDatabase(connectionId, db);
    } catch (e) {
      console.error("switchDatabase failed:", e);
    } finally {
      setSwitchingDb(false);
    }
  };

  const refresh = () => {
    useAppStore.setState((s) => ({
      schemas: Object.fromEntries(
        Object.entries(s.schemas).filter(([k]) => k !== connectionId),
      ),
    }));
  };

  const deriveTitleFromQuery = (sql: string): string | undefined => {
    try {
      const parsed = JSON.parse(sql) as Record<string, unknown>;
      const coll = (parsed.find ?? parsed.aggregate ?? parsed.insert ?? parsed.delete) as string | undefined;
      return coll ?? undefined;
    } catch {
      const m = sql.match(/\b(?:FROM|UPDATE|INTO|TABLE)\s+(?:\w+\.)?`?"?(\w+)`?"?/i);
      return m?.[1];
    }
  };

  const openQueryTab = async (sql: string, autoRun = false) => {
    if (!conn) return;
    const tabId = openTab(connectionId, conn.host, dbType, deriveTitleFromQuery(sql));
    useAppStore.setState((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, sql } : t)),
    }));
    if (autoRun) {
      useAppStore.getState().updateTab(tabId, { running: true });
      try {
        const result = await executeQuery({
          connection_id: connectionId,
          sql: sql.trim(),
        });
        useAppStore.getState().updateTab(tabId, { result, running: false });
      } catch (e) {
        useAppStore
          .getState()
          .updateTab(tabId, { error: String(e), running: false });
      }
    }
  };

  const buildContextMenuItems = (ctx: CtxMenu): ContextMenuEntry[] => {
    const fqn = ctx.schema ? `${ctx.schema}.${ctx.name}` : ctx.name;
    const isSql = [
      "postgres",
      "mysql",
      "sqlite",
      "cockroachdb",
      "mssql",
      "oracle",
      "clickhouse",
      "cassandra",
    ].includes(dbType);
    const selectQuery =
      dbType === "mongodb"
        ? JSON.stringify(
            ctx.schema
              ? { find: ctx.name, db: ctx.schema, limit: 100 }
              : { find: ctx.name, limit: 100 },
          )
        : `SELECT * FROM ${fqn} LIMIT 100;`;
    const countQuery =
      dbType === "mongodb"
        ? JSON.stringify(
            ctx.schema
              ? { aggregate: ctx.name, db: ctx.schema, pipeline: [{ $count: "count" }] }
              : { aggregate: ctx.name, pipeline: [{ $count: "count" }] },
          )
        : `SELECT COUNT(*) FROM ${fqn};`;
    const items: ContextMenuEntry[] = [
      {
        label: "Select rows",
        icon: <Table2 size={11} />,
        onClick: () => openQueryTab(selectQuery, true),
      },
      {
        label: "Count rows",
        icon: <Hash size={11} />,
        onClick: () => openQueryTab(countQuery, true),
      },
      { separator: true },
      {
        label: "Copy name",
        icon: <Eye size={11} />,
        onClick: () => {
          navigator.clipboard.writeText(ctx.name);
          useAppStore.getState().addToast(`Copied "${ctx.name}"`, "info");
        },
      },
    ];
    if (isSql && ctx.kind === "table") {
      items.push({ separator: true });
      items.push({
        label: "View ER Diagram",
        icon: <Network size={11} />,
        onClick: () => conn && openERDiagramTab(connectionId, conn.host, dbType),
      });
    }
    if (isSql) {
      items.push({ separator: true });
      if (ctx.kind === "table" || ctx.kind === "view") {
        items.push({
          label: "View DDL",
          icon: <Code2 size={11} />,
          onClick: async () => {
            if (!conn) return;
            try {
              const ddl = await getObjectDdl(connectionId, dbType, ctx.name, ctx.schema ?? null, ctx.kind);
              openDdlTab(connectionId, conn.host, dbType, ctx.name, ddl);
            } catch (e) {
              console.error("get_object_ddl failed:", e);
            }
          },
        });
        if (dbType === "postgres") {
          items.push({
            label: "Show indexes",
            onClick: () =>
              openQueryTab(
                `SELECT indexname, indexdef FROM pg_indexes WHERE tablename = '${ctx.name}';`,
              ),
          });
        }
      }
      items.push({ separator: true });
      items.push({
        label: "TRUNCATE table…",
        icon: <Trash2 size={11} />,
        danger: true,
        onClick: () =>
          setConfirmModal({
            title: `Truncate ${fqn}?`,
            sql: `TRUNCATE TABLE ${fqn};`,
          }),
      });
      items.push({
        label: "DROP table…",
        icon: <Trash2 size={11} />,
        danger: true,
        onClick: () =>
          setConfirmModal({
            title: `Drop table ${fqn}?`,
            sql: `DROP TABLE ${fqn};`,
          }),
      });
    }
    return items;
  };

  if (schema === undefined) loadSchema(connectionId);

  if (schema === "loading" || schema === undefined) {
    return (
      <div className="flex items-center gap-1.5 px-2 py-1.5 text-2xs text-text-muted">
        <RefreshCw size={10} className="animate-spin" />
        Loading schema…
      </div>
    );
  }

  if (typeof schema === "object" && "error" in schema) {
    return (
      <div className="px-2 py-1.5 text-2xs text-red-400 space-y-1">
        <div className="flex items-center gap-1.5">
          <AlertCircle size={10} className="flex-shrink-0" />
          <span className="font-medium">Schema load failed</span>
        </div>
        <div className="text-red-300 break-all pl-4">{schema.error}</div>
        <button
          className="pl-4 text-text-muted hover:text-accent underline"
          onClick={refresh}
        >
          Retry
        </button>
      </div>
    );
  }

  const dbSchema = schema as DatabaseSchema;
  const tables = dbSchema.objects.filter((o) => o.kind === "table");
  const views = dbSchema.objects.filter((o) => o.kind === "view");
  const collections = dbSchema.objects.filter((o) => o.kind === "collection");
  const keyPatterns = dbSchema.objects.filter((o) => o.kind === "key_pattern");

  // Group MongoDB collections by database name for an Atlas-style tree.
  const collectionsByDb = collections.reduce<Record<string, typeof collections>>(
    (acc, obj) => {
      const db = (obj.database as string | undefined) ?? "__default__";
      (acc[db] ??= []).push(obj);
      return acc;
    },
    {},
  );
  const isMongoGrouped = Object.keys(collectionsByDb).some((k) => k !== "__default__");

  if (
    !tables.length &&
    !views.length &&
    !collections.length &&
    !keyPatterns.length
  ) {
    return (
      <div className="py-0.5">
        {/* Keep the DB switcher visible even when empty so user can switch back */}
        {MULTI_DB_TYPES.has(dbType) && databases.length > 1 && (
          <div className="flex items-center justify-between px-2 py-0.5 mb-0.5">
            <div className="flex items-center gap-1 min-w-0">
              <Database size={9} className="text-text-muted flex-shrink-0" />
              <select
                value={conn?.database ?? dbSchema.name}
                onChange={(e) => handleSwitchDatabase(e.target.value)}
                disabled={switchingDb}
                className="text-2xs text-text-secondary bg-transparent border-none outline-none cursor-pointer truncate max-w-[120px] hover:text-accent transition-colors"
                title="Switch database"
              >
                {databases.map((db) => (
                  <option key={db} value={db}>{db}</option>
                ))}
              </select>
            </div>
            {switchingDb && <RefreshCw size={10} className="animate-spin text-text-muted" />}
          </div>
        )}
        <div className="px-2 py-1.5 text-2xs text-text-muted italic">
          No objects found
          {MULTI_DB_TYPES.has(dbType) && databases.length > 1 && (
            <div className="mt-1 not-italic text-text-muted">Use the dropdown above to switch databases.</div>
          )}
        </div>
      </div>
    );
  }

  const buildSelectSql = (name: string, database?: string): string => {
    if (dbType === "mongodb") {
      const q: Record<string, unknown> = { find: name, limit: 100 };
      if (database) q.db = database;
      return JSON.stringify(q);
    }
    return `SELECT * FROM ${name} LIMIT 100;`;
  };

  const handleTableClick = async (name: string, database?: string) => {
    if (!conn) return;
    const sql = buildSelectSql(name, database);
    const tabId = openTab(connectionId, conn.host, conn.db_type, name);
    useAppStore.setState((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, sql } : t)),
    }));
    useAppStore.getState().updateTab(tabId, { running: true });
    try {
      const result = await executeQuery({ connection_id: connectionId, sql });
      useAppStore.getState().updateTab(tabId, { result, running: false });
    } catch (e) {
      useAppStore
        .getState()
        .updateTab(tabId, { error: String(e), running: false });
    }
  };

  const onCtxMenu = (
    e: React.MouseEvent,
    obj: DatabaseSchema["objects"][number],
    kind: string,
  ) => {
    e.preventDefault();
    e.stopPropagation();
    const typedObj = obj as { schema?: string; database?: string };
    setCtxMenu({
      x: e.clientX,
      y: e.clientY,
      name: obj.name as string,
      kind,
      // For SQL objects, `schema` is the namespace prefix (e.g. "public").
      // For MongoDB collections, `schema` carries the parent database name so
      // the context-menu query builder can target the right db.
      schema: typedObj.schema ?? typedObj.database,
    });
  };

  const fqName = (obj: DatabaseSchema["objects"][number]) => {
    const s = (obj as { schema?: string }).schema;
    return s ? `${s}.${obj.name as string}` : (obj.name as string);
  };

  return (
    <div className="py-0.5">
      <div className="flex items-center justify-between px-2 py-0.5 mb-0.5">
        <div className="flex items-center gap-1 min-w-0">
          <Database size={9} className="text-text-muted flex-shrink-0" />
          {MULTI_DB_TYPES.has(dbType) && databases.length > 1 ? (
            <select
              value={conn?.database ?? dbSchema.name}
              onChange={(e) => handleSwitchDatabase(e.target.value)}
              disabled={switchingDb}
              className="text-2xs text-text-secondary bg-transparent border-none outline-none cursor-pointer truncate max-w-[120px] hover:text-accent transition-colors"
              title="Switch database"
            >
              {databases.map((db) => (
                <option key={db} value={db}>{db}</option>
              ))}
            </select>
          ) : (
            <span className="text-2xs text-text-muted truncate" title={dbSchema.name}>
              {dbSchema.name}
            </span>
          )}
          <span className="text-2xs text-text-muted flex-shrink-0">
            ({tables.length + views.length + collections.length + keyPatterns.length})
          </span>
        </div>
        <button
          className="text-text-muted hover:text-accent transition-colors p-0.5 flex-shrink-0"
          onClick={refresh}
          title="Refresh schema"
        >
          {switchingDb ? <RefreshCw size={10} className="animate-spin" /> : <RefreshCw size={10} />}
        </button>
      </div>

      {tables.map((obj) => (
        <SchemaObjectRow
          key={`t-${obj.name as string}`}
          icon={<Table2 size={10} />}
          obj={obj}
          onClick={() => handleTableClick(fqName(obj))}
          onContextMenu={(e) => onCtxMenu(e, obj, "table")}
        />
      ))}
      {views.map((obj) => (
        <SchemaObjectRow
          key={`v-${obj.name as string}`}
          icon={<Eye size={10} />}
          obj={obj}
          onClick={() => handleTableClick(fqName(obj))}
          onContextMenu={(e) => onCtxMenu(e, obj, "view")}
        />
      ))}
      {isMongoGrouped
        ? Object.entries(collectionsByDb).map(([db, colls]) => (
            <MongoDbGroup
              key={db}
              dbName={db}
              colls={colls}
              onCollClick={(name, database) => handleTableClick(name, database)}
              onCtxMenu={onCtxMenu}
            />
          ))
        : collections.map((obj) => (
            <SchemaObjectRow
              key={`c-${obj.name as string}`}
              icon={<Table2 size={10} />}
              obj={obj}
              onClick={() =>
                handleTableClick(
                  obj.name as string,
                  (obj.database as string | undefined),
                )
              }
              onContextMenu={(e) => onCtxMenu(e, obj, "collection")}
            />
          ))}
      {keyPatterns.map((obj) => (
        <SchemaObjectRow
          key={`k-${(obj as { pattern?: string }).pattern ?? obj.name}`}
          icon={<Eye size={10} />}
          obj={obj}
          onClick={() => {}}
          onContextMenu={(e) => onCtxMenu(e, obj, "key_pattern")}
        />
      ))}

      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          items={buildContextMenuItems(ctxMenu)}
          onClose={() => setCtxMenu(null)}
        />
      )}

      {confirmModal && (
        <DangerConfirm
          title={confirmModal.title}
          sql={confirmModal.sql}
          onConfirm={async () => {
            try {
              await executeQuery({
                connection_id: connectionId,
                sql: confirmModal.sql,
              });
              useAppStore.setState((s) => ({
                schemas: Object.fromEntries(
                  Object.entries(s.schemas).filter(([k]) => k !== connectionId),
                ),
              }));
              const { tabs, activeTabId } = useAppStore.getState();
              const activeTab = tabs.find(
                (t) => t.id === activeTabId && t.connection_id === connectionId,
              );
              if (activeTab?.sql.trim()) {
                useAppStore
                  .getState()
                  .updateTab(activeTab.id, {
                    running: true,
                    result: undefined,
                  });
                try {
                  const refreshed = await executeQuery({
                    connection_id: connectionId,
                    sql: activeTab.sql.trim(),
                  });
                  useAppStore
                    .getState()
                    .updateTab(activeTab.id, {
                      result: refreshed,
                      running: false,
                    });
                } catch {
                  useAppStore
                    .getState()
                    .updateTab(activeTab.id, { running: false });
                }
              }
            } catch (e) {
              console.error("Operation failed:", e);
            }
            setConfirmModal(null);
          }}
          onCancel={() => setConfirmModal(null)}
        />
      )}
    </div>
  );
}
