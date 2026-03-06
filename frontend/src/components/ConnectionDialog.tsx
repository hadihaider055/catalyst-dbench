import { useState } from "react";
import { X, TestTube, Save, Eye, EyeOff, Lock } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import { testConnection } from "@/lib/commands";
import { cn, generateId, dbIcon } from "@/lib/utils";
import type { DatabaseType, SavedConnection } from "@/lib/types";
import { DB_LABELS, DB_DEFAULTS } from "@/lib/types";

const SUPPORTED: DatabaseType[] = [
  "postgres", "mysql", "sqlite", "mongodb", "redis",
  "cockroachdb", "clickhouse", "cassandra",
];

interface Props { onClose: () => void; existing?: SavedConnection }

export default function ConnectionDialog({ onClose, existing }: Props) {
  const { upsertSavedConnection } = useAppStore();

  const [dbType, setDbType] = useState<DatabaseType>(existing?.db_type ?? "postgres");
  const [name, setName] = useState(existing?.name ?? "");
  const [host, setHost] = useState(existing?.host ?? "localhost");
  const [port, setPort] = useState(String(existing?.port ?? DB_DEFAULTS[dbType] ?? 5432));
  const [database, setDatabase] = useState(existing?.database ?? "");
  const [username, setUsername] = useState(existing?.username ?? "");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [tls, setTls] = useState(existing?.tls_enabled ?? true);
  const [readOnly, setReadOnly] = useState(existing?.read_only ?? false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const handleDbTypeChange = (t: DatabaseType) => {
    setDbType(t);
    setPort(String(DB_DEFAULTS[t] ?? ""));
    setTestResult(null);
  };

  const handleTest = async () => {
    setTesting(true); setTestResult(null);
    try {
      const msg = await testConnection({ name, db_type: dbType, host, port: parseInt(port), database, username, password, tls_enabled: tls, read_only: readOnly });
      setTestResult({ ok: true, msg });
    } catch (e) {
      setTestResult({ ok: false, msg: String(e) });
    }
    setTesting(false);
  };

  const handleSave = () => {
    const conn: SavedConnection = {
      id: existing?.id ?? generateId(),
      name: name || `${dbType}-${host}`,
      db_type: dbType,
      host,
      port: parseInt(port) || (DB_DEFAULTS[dbType] ?? 5432),
      database,
      username,
      tls_enabled: tls,
      read_only: readOnly,
    };
    upsertSavedConnection(conn);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-surface-raised border border-surface-border rounded-lg w-[520px] shadow-2xl flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-surface-border">
          <h2 className="text-sm font-semibold text-text-primary">
            {existing ? "Edit Connection" : "New Connection"}
          </h2>
          <button className="text-text-muted hover:text-text-primary transition-colors" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          {/* DB type picker */}
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
                      : "border-surface-border hover:border-text-muted text-text-muted hover:text-text-secondary"
                  )}
                  onClick={() => handleDbTypeChange(t)}
                >
                  <span className="text-lg">{dbIcon(t)}</span>
                  <span className="truncate w-full text-center">{DB_LABELS[t]?.split(" / ")[0] ?? t}</span>
                </button>
              ))}
            </div>
          </div>

          {/* Name */}
          <Field label="Connection Name" hint="Display name">
            <Input value={name} onChange={setName} placeholder={`${dbType}-${host}`} />
          </Field>

          {dbType !== "sqlite" ? (
            <>
              <div className="grid grid-cols-3 gap-3">
                <div className="col-span-2">
                  <Field label="Host">
                    <Input value={host} onChange={setHost} placeholder="localhost" />
                  </Field>
                </div>
                <Field label="Port">
                  <Input value={port} onChange={setPort} placeholder="5432" type="number" />
                </Field>
              </div>

              <Field label="Database">
                <Input value={database} onChange={setDatabase} placeholder="mydb" />
              </Field>

              <div className="grid grid-cols-2 gap-3">
                <Field label="Username">
                  <Input value={username} onChange={setUsername} placeholder="postgres" />
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
          ) : (
            <Field label="Database File Path">
              <Input value={database} onChange={setDatabase} placeholder="/path/to/db.sqlite or :memory:" />
            </Field>
          )}

          {/* Options */}
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

          {/* Test result */}
          {testResult && (
            <div className={cn(
              "text-xs px-3 py-2 rounded border",
              testResult.ok
                ? "bg-green-950 border-green-800 text-green-300"
                : "bg-red-950 border-red-800 text-red-300"
            )}>
              {testResult.msg}
            </div>
          )}
        </div>

        {/* Footer */}
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
              className="flex items-center gap-1.5 px-4 py-1.5 bg-accent hover:bg-accent-hover text-white rounded text-xs font-medium transition-colors"
              onClick={handleSave}
            >
              <Save size={12} />
              Save Connection
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
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
  value, onChange, placeholder, type = "text",
}: {
  value: string; onChange: (v: string) => void; placeholder?: string; type?: string;
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
