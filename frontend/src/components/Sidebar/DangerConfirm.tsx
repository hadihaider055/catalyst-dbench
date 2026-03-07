import { AlertCircle } from "lucide-react";

interface Props {
  title: string;
  sql: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function DangerConfirm({ title, sql, confirmLabel = "Execute", onConfirm, onCancel }: Props) {
  const isSql = sql.trim().toUpperCase().startsWith("TRUNCATE") ||
                sql.trim().toUpperCase().startsWith("DROP") ||
                sql.trim().toUpperCase().startsWith("DELETE");
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div
        className="bg-surface-raised border border-red-800/60 rounded-lg w-[480px] shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-4 py-3 border-b border-red-800/40">
          <AlertCircle size={14} className="text-red-400 flex-shrink-0" />
          <span className="text-sm font-semibold text-red-400">{title}</span>
        </div>
        <div className="px-4 py-3">
          {isSql ? (
            <>
              <p className="text-xs text-text-muted mb-2">This will execute:</p>
              <pre className="text-xs text-red-300 font-mono bg-surface border border-surface-border rounded px-3 py-2">{sql}</pre>
            </>
          ) : (
            <p className="text-xs text-text-secondary">{sql}</p>
          )}
          <p className="text-xs text-text-muted mt-2">This action cannot be undone.</p>
        </div>
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-surface-border">
          <button
            className="px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary border border-surface-border rounded transition-colors"
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            className="px-3 py-1.5 text-xs text-white bg-red-700 hover:bg-red-600 rounded transition-colors"
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
