import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { displayValue } from "@/lib/types";
import type { QueryResult, Row, Column, Value } from "@/lib/types";
import { useAppStore } from "@/stores/useAppStore";

function useJsonColors() {
  const theme = useAppStore((s) => s.theme);
  return {
    string: theme === "dark" ? "text-yellow-300" : "text-amber-700",
    number: theme === "dark" ? "text-green-400" : "text-green-700",
    bool: theme === "dark" ? "text-blue-400" : "text-blue-700",
    null: "text-text-muted italic",
  };
}

interface Props {
  result: QueryResult;
}

export default function MongoDocTree({ result }: Props) {
  if (result.rows.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-text-muted">
        No documents
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto p-2 space-y-1.5">
      {result.rows.map((row, i) => (
        <DocCard key={i} index={i} row={row} columns={result.columns} />
      ))}
    </div>
  );
}

function DocCard({ index, row, columns }: { index: number; row: Row; columns: Column[] }) {
  const [collapsed, setCollapsed] = useState(false);
  return (
    <div className="border border-surface-border rounded bg-surface-raised text-xs">
      <div
        className="flex items-center gap-1.5 px-3 py-1.5 cursor-pointer hover:bg-surface-overlay select-none border-b border-surface-border"
        onClick={() => setCollapsed((c) => !c)}
      >
        <ChevronRight
          size={10}
          className={cn("text-text-muted transition-transform", !collapsed && "rotate-90")}
        />
        <span className="text-text-muted font-mono">Document #{index + 1}</span>
      </div>
      {!collapsed && (
        <div className="p-2 space-y-0.5">
          {columns.map((col, i) => (
            <DocField
              key={col.name}
              name={col.name}
              value={(row.values[i] ?? { type: "null" }) as Value}
              nativeType={col.native_type}
            />
          ))}
        </div>
      )}
    </div>
  );
}

const TYPE_COLORS_DARK: Record<string, string> = {
  ObjectId: "bg-purple-950/50 text-purple-400",
  String: "bg-yellow-950/50 text-yellow-400",
  text: "bg-yellow-950/50 text-yellow-400",
  Int32: "bg-green-950/50 text-green-400",
  Int64: "bg-green-950/50 text-green-400",
  Double: "bg-green-950/50 text-green-400",
  int: "bg-green-950/50 text-green-400",
  float: "bg-green-950/50 text-green-400",
  decimal: "bg-green-950/50 text-green-400",
  Boolean: "bg-blue-950/50 text-blue-400",
  bool: "bg-blue-950/50 text-blue-400",
  Date: "bg-cyan-950/50 text-cyan-400",
  date: "bg-cyan-950/50 text-cyan-400",
  timestamp: "bg-cyan-950/50 text-cyan-400",
  Array: "bg-orange-950/50 text-orange-400",
  array: "bg-orange-950/50 text-orange-400",
  Document: "bg-indigo-950/50 text-indigo-400",
  Object: "bg-indigo-950/50 text-indigo-400",
  object: "bg-indigo-950/50 text-indigo-400",
  json: "bg-indigo-950/50 text-indigo-400",
  Null: "bg-surface-overlay text-text-muted",
  null: "bg-surface-overlay text-text-muted",
};

const TYPE_COLORS_LIGHT: Record<string, string> = {
  ObjectId: "bg-purple-100 text-purple-700",
  String: "bg-amber-100 text-amber-700",
  text: "bg-amber-100 text-amber-700",
  Int32: "bg-green-100 text-green-700",
  Int64: "bg-green-100 text-green-700",
  Double: "bg-green-100 text-green-700",
  int: "bg-green-100 text-green-700",
  float: "bg-green-100 text-green-700",
  decimal: "bg-green-100 text-green-700",
  Boolean: "bg-blue-100 text-blue-700",
  bool: "bg-blue-100 text-blue-700",
  Date: "bg-cyan-100 text-cyan-700",
  date: "bg-cyan-100 text-cyan-700",
  timestamp: "bg-cyan-100 text-cyan-700",
  Array: "bg-orange-100 text-orange-700",
  array: "bg-orange-100 text-orange-700",
  Document: "bg-indigo-100 text-indigo-700",
  Object: "bg-indigo-100 text-indigo-700",
  object: "bg-indigo-100 text-indigo-700",
  json: "bg-indigo-100 text-indigo-700",
  Null: "bg-surface-overlay text-text-muted",
  null: "bg-surface-overlay text-text-muted",
};

function TypeBadge({ type }: { type: string }) {
  const theme = useAppStore((s) => s.theme);
  const colors = theme === "dark" ? TYPE_COLORS_DARK : TYPE_COLORS_LIGHT;
  return (
    <span
      className={cn(
        "text-2xs px-1 py-0 rounded font-mono flex-shrink-0",
        colors[type] ?? "bg-surface-overlay text-text-muted",
      )}
    >
      {type}
    </span>
  );
}

function DocField({ name, value, nativeType }: { name: string; value: Value; nativeType: string }) {
  const colors = useJsonColors();
  const isExpandable =
    value.type === "object" || value.type === "json" || value.type === "array";

  if (isExpandable) {
    return <ExpandableField name={name} value={value} nativeType={nativeType} />;
  }

  const text = displayValue(value);
  return (
    <div className="flex items-start gap-2 py-0.5 pl-4">
      <span className="text-text-secondary font-mono min-w-28 flex-shrink-0 truncate">{name}</span>
      <TypeBadge type={nativeType || value.type} />
      <span
        className={cn(
          "font-mono text-text-primary truncate max-w-xs",
          value.type === "null" && colors.null,
          value.type === "text" && colors.string,
          (value.type === "int" || value.type === "float") && colors.number,
          value.type === "bool" && colors.bool,
        )}
        title={text}
      >
        {value.type === "text" ? `"${text}"` : text}
      </span>
    </div>
  );
}

function ExpandableField({
  name,
  value,
  nativeType,
}: {
  name: string;
  value: Value;
  nativeType: string;
}) {
  const [expanded, setExpanded] = useState(false);

  const preview = (() => {
    if (value.type === "array") return `[ ${value.v.length} items ]`;
    if (value.type === "object") return `{ ${Object.keys(value.v).length} fields }`;
    if (value.type === "json") {
      const v = value.v;
      if (Array.isArray(v)) return `[ ${v.length} items ]`;
      if (v && typeof v === "object") return `{ ${Object.keys(v as object).length} fields }`;
      return String(v);
    }
    return "{ ... }";
  })();

  const innerContent = (() => {
    if (value.type === "array") {
      return (
        <div className="space-y-0.5">
          {value.v.map((item, i) => (
            <DocField
              key={i}
              name={`[${i}]`}
              value={item}
              nativeType={item.type}
            />
          ))}
        </div>
      );
    }
    if (value.type === "object") {
      return <JsonTree value={value.v} />;
    }
    if (value.type === "json") {
      return <JsonTree value={value.v} />;
    }
    return null;
  })();

  return (
    <div className="py-0.5">
      <div
        className="flex items-center gap-2 pl-4 cursor-pointer hover:text-text-primary"
        onClick={() => setExpanded((e) => !e)}
      >
        <ChevronRight
          size={10}
          className={cn("text-text-muted transition-transform", expanded && "rotate-90")}
        />
        <span className="text-text-secondary font-mono min-w-24 flex-shrink-0 truncate">{name}</span>
        <TypeBadge type={nativeType || value.type} />
        {!expanded && <span className="text-text-muted font-mono">{preview}</span>}
      </div>
      {expanded && (
        <div className="ml-8 border-l border-surface-border pl-2 mt-0.5">{innerContent}</div>
      )}
    </div>
  );
}

function JsonTree({ value }: { value: unknown }) {
  const colors = useJsonColors();
  if (value === null || value === undefined) {
    return <span className={cn("font-mono text-xs pl-4", colors.null)}>null</span>;
  }
  if (typeof value === "boolean") {
    return <span className={cn("font-mono text-xs pl-4", colors.bool)}>{String(value)}</span>;
  }
  if (typeof value === "number") {
    return <span className={cn("font-mono text-xs pl-4", colors.number)}>{value}</span>;
  }
  if (typeof value === "string") {
    return <span className={cn("font-mono text-xs pl-4", colors.string)}>"{value}"</span>;
  }
  if (Array.isArray(value)) {
    return (
      <div className="space-y-0.5">
        {(value as unknown[]).map((item, i) => (
          <div key={i} className="flex items-start gap-2 py-0.5 pl-4">
            <span className="text-text-muted font-mono text-xs min-w-8 flex-shrink-0">[{i}]</span>
            <JsonTree value={item} />
          </div>
        ))}
      </div>
    );
  }
  if (typeof value === "object") {
    return (
      <div className="space-y-0.5">
        {Object.entries(value as Record<string, unknown>).map(([k, v]) => (
          <div key={k} className="flex items-start gap-2 py-0.5 pl-4">
            <span className="text-text-secondary font-mono text-xs min-w-24 flex-shrink-0 truncate">
              {k}:
            </span>
            <JsonTree value={v} />
          </div>
        ))}
      </div>
    );
  }
  return <span className="text-text-primary font-mono text-xs pl-4">{String(value)}</span>;
}
