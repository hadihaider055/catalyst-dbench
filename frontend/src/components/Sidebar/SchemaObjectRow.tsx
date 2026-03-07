import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { DatabaseSchema } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Props {
  icon: React.ReactNode;
  obj: DatabaseSchema["objects"][number];
  onClick: () => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}

type Column = { name: string; native_type: string; is_primary_key?: boolean };

export default function SchemaObjectRow({ icon, obj, onClick, onContextMenu }: Props) {
  const [open, setOpen] = useState(false);
  const columns: Column[] = (obj as { columns?: Column[] }).columns ?? [];
  const name = obj.name as string;

  return (
    <div>
      <div
        className="flex items-center gap-1.5 px-2 py-1 cursor-pointer hover:bg-surface-overlay rounded text-2xs text-text-secondary"
        onClick={() => { setOpen((o) => !o); onClick(); }}
        onContextMenu={onContextMenu}
      >
        <button
          className="text-text-muted flex-shrink-0"
          onClick={(e) => { e.stopPropagation(); setOpen((o) => !o); }}
        >
          {open ? <ChevronDown size={9} /> : <ChevronRight size={9} />}
        </button>
        <span className="text-text-muted flex-shrink-0">{icon}</span>
        <span className="truncate flex-1">{name}</span>
        {columns.length > 0 && (
          <span className="text-text-muted flex-shrink-0 ml-1">{columns.length}</span>
        )}
      </div>
      {open && columns.length > 0 && (
        <div className="pl-6 py-0.5">
          {columns.map((col) => (
            <div key={col.name} className={cn("flex items-center gap-1.5 px-2 py-0.5 text-2xs text-text-muted")}>
              {col.is_primary_key && <span className="text-yellow-500 text-2xs font-bold">PK</span>}
              <span className="text-text-secondary">{col.name}</span>
              <span className="ml-auto text-text-muted truncate max-w-16">{col.native_type}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
