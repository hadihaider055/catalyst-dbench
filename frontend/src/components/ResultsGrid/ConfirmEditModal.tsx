import { useState } from "react";

// Lucide icons
import { X, Save } from "lucide-react";

interface Props {
  sql: string;
  error: string | null;
  onConfirm: (sql: string) => void;
  onCancel: () => void;
}

export default function ConfirmEditModal({
  sql,
  error,
  onConfirm,
  onCancel,
}: Props) {
  const [editedSql, setEditedSql] = useState(sql);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div
        className="bg-surface-raised border border-surface-border rounded-lg w-[640px] max-h-[80vh] flex flex-col shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-surface-border">
          <span className="text-xs font-semibold text-text-primary">
            Review changes before applying
          </span>
          <button
            className="text-text-muted hover:text-text-primary"
            onClick={onCancel}
          >
            <X size={14} />
          </button>
        </div>
        <div className="flex-1 overflow-auto p-4">
          <p className="text-2xs text-text-muted mb-2">
            Review and edit the SQL, then click Apply:
          </p>
          <textarea
            className="w-full bg-surface border border-surface-border rounded px-3 py-2 text-xs text-text-primary font-mono outline-none focus:border-accent resize-none"
            rows={Math.min(14, editedSql.split("\n").length + 2)}
            value={editedSql}
            onChange={(e) => setEditedSql(e.target.value)}
          />
          {error && (
            <div className="mt-2 px-3 py-2 bg-red-950/60 border border-red-800 rounded text-xs text-red-300">
              {error}
            </div>
          )}
        </div>
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-surface-border">
          <button
            className="px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary border border-surface-border rounded transition-colors"
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            className="flex items-center gap-1.5 px-4 py-1.5 text-xs text-white bg-accent hover:bg-accent-hover rounded transition-colors"
            onClick={() => onConfirm(editedSql)}
          >
            <Save size={11} /> Apply
          </button>
        </div>
      </div>
    </div>
  );
}
