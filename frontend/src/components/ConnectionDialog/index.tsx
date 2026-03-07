import { useState, useEffect } from "react";
import { X, TestTube, Save, Eye, EyeOff, Lock, Link } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { addConnection, testConnection, storeCredential, getCredential } from "@/lib/commands";
import { cn, dbIcon } from "@/lib/utils";
import type { DatabaseType, SavedConnection } from "@/lib/types";
import { DB_LABELS, DB_DEFAULTS } from "@/lib/types";

const SUPPORTED: DatabaseType[] = [
  "postgres",
  "mysql",
  "sqlite",
  "mongodb",
  "redis",
  "cockroachdb",
  "clickhouse",
  "cassandra",
];

const URI_CAPABLE: DatabaseType[] = ["mongodb", "postgres", "mysql", "redis"];

interface Props {
  onClose: () => void;
  existing?: SavedConnection;
  reconnect?: boolean;
}

export default function ConnectionDialog({ onClose, existing, reconnect }: Props) {
  const { upsertSavedConnection, removeSavedConnection, addActiveConnection, openTab } =
    useAppStore();

  const [dbType, setDbType] = useState<DatabaseType>(existing?.db_type ?? "postgres");
  const [name, setName] = useState(existing?.name ?? "");
  const initialHost = existing?.host ?? "localhost";
  const [host, setHost] = useState(initialHost);
  const [port, setPort] = useState(String(existing?.port ?? DB_DEFAULTS[dbType] ?? 5432));
  const [database, setDatabase] = useState(existing?.database ?? "");
  const [username, setUsername] = useState(existing?.username ?? "");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [uri, setUri] = useState("");
  const [useUri, setUseUri] = useState(false);

  const [tls, setTls] = useState(existing?.tls_enabled ?? false);
  const [readOnly, setReadOnly] = useState(existing?.read_only ?? false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  // On reconnect, pre-fill password from the OS keychain so the user doesn't have to re-enter it.
  useEffect(() => {
    if (reconnect && existing) {
      getCredential(existing.id).then((pwd) => {
        if (pwd) setPassword(pwd);
      }).catch(() => {/* no stored credential — leave empty */});
    }
  }, [reconnect, existing?.id]);

  const handleDbTypeChange = (t: DatabaseType) => {
    setDbType(t);
    setPort(String(DB_DEFAULTS[t] ?? ""));
    setUseUri(false);
    setTestResult(null);
  };

  const handleHostChange = (h: string) => {
    setHost(h);
  };

  const buildPayload = () => {
    const connName = name || `${dbType}-${host}`;
    if (useUri && uri) {
      return {
        name: connName,
        db_type: dbType,
        host: uri,
        port: undefined as number | undefined,
        database,
        username: "",
        password: password || undefined,
        tls_enabled: tls,
        read_only: readOnly,
      };
    }
    return {
      name: connName,
      db_type: dbType,
      host,
      port: parseInt(port) || DB_DEFAULTS[dbType] || 5432,
      database,
      username,
      password: password || undefined,
      tls_enabled: tls,
      read_only: readOnly,
    };
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const msg = await testConnection(buildPayload());
      setTestResult({ ok: true, msg });
    } catch (e) {
      setTestResult({ ok: false, msg: String(e) });
    }
    setTesting(false);
  };

  const handleSave = async () => {
    const payload = buildPayload();

    if (reconnect || !existing) {
      setTesting(true);
      setTestResult(null);
      try {
        const result = await addConnection(payload);
        if (existing && existing.id !== result.info.id) removeSavedConnection(existing.id);
        const conn: SavedConnection = {
          id: result.info.id,
          name: payload.name,
          db_type: dbType,
          host: useUri ? uri : host,
          port: result.info.port,
          database,
          username,
          tls_enabled: tls,
          read_only: readOnly,
        };
        upsertSavedConnection(conn);
        addActiveConnection(result.info);
        openTab(result.info.id, conn.name, conn.db_type);
        // Persist the password in the OS keychain so reconnect works without re-entry.
        if (payload.password) {
          storeCredential(result.info.id, payload.password).catch(() => {/* best-effort */});
        }
        onClose();
      } catch (e) {
        setTestResult({ ok: false, msg: `Connection failed: ${String(e)}` });
        setTesting(false);
      }
      return;
    }

    const conn: SavedConnection = {
      id: existing.id,
      name: payload.name,
      db_type: dbType,
      host: useUri ? uri : host,
      port: parseInt(port) || DB_DEFAULTS[dbType] || 5432,
      database,
      username,
      tls_enabled: tls,
      read_only: readOnly,
    };
    upsertSavedConnection(conn);
    onClose();
  };

  const canUseUri = URI_CAPABLE.includes(dbType);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-surface-raised border border-surface-border rounded-lg w-[520px] shadow-2xl flex flex-col max-h-[90vh]">
        <div className="flex items-center justify-between px-5 py-4 border-b border-surface-border">
          <h2 className="text-sm font-semibold text-text-primary">
            {reconnect ? "Connect" : existing ? "Edit Connection" : "New Connection"}
          </h2>
          <button
            className="text-text-muted hover:text-text-primary transition-colors"
            onClick={onClose}
          >
            <X size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          <div>
            <label className="block text-xs text-text-secondary mb-2">Database Type</label>
            <div className="grid grid-cols-4 gap-2">
              {SUPPORTED.map((t) => (
                <button
                  key={t}
                  className={cn(
                    "flex flex-col items-center gap-1 py-2.5 px-2 rounded border text-xs transition-all",
                    dbType === t
                      ? "border-accent bg-accent-muted text-text-primary"
                      : "border-surface-border hover:border-text-muted text-text-muted hover:text-text-secondary",
                  )}
                  onClick={() => handleDbTypeChange(t)}
                >
                  <span className="text-lg">{dbIcon(t)}</span>
                  <span className="truncate w-full text-center">
                    {DB_LABELS[t]?.split(" / ")[0] ?? t}
                  </span>
                </button>
              ))}
            </div>
          </div>

          <Field label="Connection Name" hint="Display name">
            <Input
              value={name}
              onChange={setName}
              placeholder={`${dbType}-${useUri ? "conn" : host}`}
            />
          </Field>

          {dbType === "sqlite" ? (
            <Field label="Database File Path">
              <Input
                value={database}
                onChange={setDatabase}
                placeholder="/path/to/db.sqlite or :memory:"
              />
            </Field>
          ) : (
            <>
              {canUseUri && (
                <div className="flex items-center gap-2">
                  <button
                    className={cn(
                      "flex items-center gap-1.5 px-2.5 py-1 rounded border text-2xs transition-colors",
                      useUri
                        ? "border-accent bg-accent-muted text-accent"
                        : "border-surface-border text-text-muted hover:text-text-secondary hover:border-text-muted",
                    )}
                    onClick={() => setUseUri((v) => !v)}
                  >
                    <Link size={10} />
                    {dbType === "mongodb"
                      ? "Use connection string (mongodb+srv://…)"
                      : "Use connection URI"}
                  </button>
                </div>
              )}

              {useUri ? (
                <>
                  <Field label="Connection URI">
                    <Input
                      value={uri}
                      onChange={setUri}
                      placeholder={
                        dbType === "mongodb"
                          ? "mongodb+srv://user:pass@cluster.example.mongodb.net/mydb"
                          : dbType === "redis"
                            ? "redis://user:pass@host:6379/0"
                            : "postgresql://user:pass@host:5432/mydb"
                      }
                    />
                    {dbType === "mongodb" && (
                      <p className="text-2xs text-text-muted mt-1">
                        Include the database in the URI path (e.g. <code className="font-mono">/mydb</code>) or fill the field below. SRV URIs resolve the port from DNS — no port needed.
                      </p>
                    )}
                  </Field>

                  <Field label="Database" hint="optional if included in URI">
                    <Input
                      value={database}
                      onChange={setDatabase}
                      placeholder="mydb"
                    />
                  </Field>
                </>
              ) : (
                <>
                  <div className="grid grid-cols-3 gap-3">
                    <div className="col-span-2">
                      <Field label="Host">
                        <Input
                          value={host}
                          onChange={handleHostChange}
                          placeholder="localhost"
                        />
                      </Field>
                    </div>
                    <Field label="Port">
                      <Input
                        value={port}
                        onChange={setPort}
                        placeholder={String(DB_DEFAULTS[dbType] ?? 5432)}
                        type="number"
                      />
                    </Field>
                  </div>

                  <Field label={dbType === "redis" ? "Key prefix / DB index" : "Database"}>
                    <Input
                      value={database}
                      onChange={setDatabase}
                      placeholder={dbType === "redis" ? "0" : "mydb"}
                    />
                  </Field>

                  <div className="grid grid-cols-2 gap-3">
                    <Field label="Username">
                      <Input
                        value={username}
                        onChange={setUsername}
                        placeholder={dbType === "postgres" ? "postgres" : "root"}
                      />
                    </Field>
                    <Field label="Password">
                      <div className="relative">
                        <Input
                          value={password}
                          onChange={setPassword}
                          placeholder="••••••••"
                          type={showPassword ? "text" : "password"}
                        />
                        <button
                          className="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-secondary"
                          onClick={() => setShowPassword((s) => !s)}
                        >
                          {showPassword ? <EyeOff size={13} /> : <Eye size={13} />}
                        </button>
                      </div>
                      <p className="text-2xs text-text-muted mt-1 flex items-center gap-1">
                        <Lock size={9} /> Stored in OS keychain, never in config files
                      </p>
                    </Field>
                  </div>
                </>
              )}

              <div className="flex items-center gap-4">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={tls}
                    onChange={(e) => setTls(e.target.checked)}
                    className="accent-accent"
                  />
                  <span className="text-xs text-text-secondary">TLS / SSL</span>
                </label>
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={readOnly}
                    onChange={(e) => setReadOnly(e.target.checked)}
                    className="accent-yellow-500"
                  />
                  <span className="text-xs text-text-secondary">Read-only mode</span>
                </label>
              </div>
            </>
          )}

          {testResult && (
            <div
              className={cn(
                "text-xs px-3 py-2 rounded border",
                testResult.ok
                  ? "bg-green-950 border-green-800 text-green-300"
                  : "bg-red-950 border-red-800 text-red-300",
              )}
            >
              {testResult.msg}
            </div>
          )}
        </div>

        <div className="flex items-center justify-between px-5 py-4 border-t border-surface-border">
          <button
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary border border-surface-border hover:border-text-muted rounded transition-colors disabled:opacity-50"
            onClick={handleTest}
            disabled={testing}
          >
            <TestTube size={12} />
            {testing ? "Testing…" : "Test Connection"}
          </button>
          <div className="flex items-center gap-2">
            <button
              className="px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors"
              onClick={onClose}
            >
              Cancel
            </button>
            <button
              className="flex items-center gap-1.5 px-4 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-xs font-medium transition-colors disabled:opacity-50"
              onClick={handleSave}
              disabled={testing}
            >
              <Save size={12} />
              {reconnect ? "Connect" : existing ? "Save Changes" : "Save & Connect"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="block text-xs text-text-secondary mb-1.5">
        {label} {hint && <span className="text-text-muted">— {hint}</span>}
      </label>
      {children}
    </div>
  );
}

function Input({
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <input
      type={type}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="w-full bg-surface border border-surface-border rounded px-3 py-1.5 text-xs text-text-primary placeholder-text-muted outline-none focus:border-accent transition-colors"
    />
  );
}
