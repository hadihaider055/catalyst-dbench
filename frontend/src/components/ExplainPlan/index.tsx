import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

// ── PostgreSQL EXPLAIN (FORMAT JSON) types ────────────────────────────────────

interface PgPlanNode {
  "Node Type": string;
  "Relation Name"?: string;
  "Alias"?: string;
  "Startup Cost"?: number;
  "Total Cost"?: number;
  "Plan Rows"?: number;
  "Actual Rows"?: number;
  "Actual Total Time"?: number;
  "Actual Loops"?: number;
  "Index Name"?: string;
  "Index Cond"?: string;
  "Filter"?: string;
  "Join Type"?: string;
  "Hash Cond"?: string;
  "Plans"?: PgPlanNode[];
  [key: string]: unknown;
}

interface PgExplainResult {
  Plan: PgPlanNode;
  "Planning Time"?: number;
  "Execution Time"?: number;
}

// ── Plan node tree ────────────────────────────────────────────────────────────

function PlanNode({ node, depth = 0 }: { node: PgPlanNode; depth?: number }) {
  const [open, setOpen] = useState(true);
  const hasChildren = node.Plans && node.Plans.length > 0;

  const costClass =
    (node["Total Cost"] ?? 0) > 1000
      ? "text-red-400"
      : (node["Total Cost"] ?? 0) > 100
        ? "text-yellow-400"
        : "text-green-400";

  const label = [
    node["Node Type"],
    node["Relation Name"] && `on ${node["Relation Name"]}`,
    node["Alias"] && `(${node["Alias"]})`,
    node["Join Type"] && `[${node["Join Type"]}]`,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div style={{ paddingLeft: depth > 0 ? 16 : 0 }} className="text-xs">
      <div
        className={cn(
          "flex items-start gap-1.5 py-0.5 px-1 rounded hover:bg-surface-overlay cursor-default",
        )}
        onClick={() => hasChildren && setOpen((v) => !v)}
      >
        {hasChildren ? (
          <ChevronRight
            size={10}
            className={cn("mt-0.5 flex-shrink-0 transition-transform text-text-muted", open && "rotate-90")}
          />
        ) : (
          <span className="w-2.5 flex-shrink-0" />
        )}

        <span className="font-medium text-text-primary">{label}</span>

        <span className={cn("ml-auto flex-shrink-0 tabular-nums", costClass)}>
          {node["Total Cost"] !== undefined && `cost=${node["Startup Cost"]?.toFixed(2)}..${node["Total Cost"]?.toFixed(2)}`}
        </span>

        {node["Actual Total Time"] !== undefined && (
          <span className="text-text-muted tabular-nums flex-shrink-0">
            {node["Actual Total Time"].toFixed(3)} ms
          </span>
        )}

        {node["Plan Rows"] !== undefined && (
          <span className="text-text-muted tabular-nums flex-shrink-0">
            rows={node["Actual Rows"] !== undefined
              ? `${node["Actual Rows"]}/${node["Plan Rows"]}`
              : node["Plan Rows"]}
          </span>
        )}
      </div>

      {/* Details */}
      {open && (
        <div className="pl-4 text-2xs text-text-muted space-y-0.5">
          {node["Index Name"] && <div>Index: {node["Index Name"]}</div>}
          {node["Index Cond"] && <div>Cond: {node["Index Cond"]}</div>}
          {node["Filter"] && <div>Filter: {node["Filter"]}</div>}
          {node["Hash Cond"] && <div>Hash: {node["Hash Cond"]}</div>}
        </div>
      )}

      {/* Children */}
      {open && node.Plans?.map((child, i) => (
        <PlanNode key={i} node={child} depth={depth + 1} />
      ))}
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export default function ExplainPlan({ plan }: { plan?: string }) {
  if (!plan) return null;

  // Try to parse as PostgreSQL JSON explain
  let parsed: PgExplainResult[] | null = null;
  try {
    const raw = JSON.parse(plan) as unknown;
    if (Array.isArray(raw) && raw.length > 0 && (raw[0] as PgExplainResult).Plan) {
      parsed = raw as PgExplainResult[];
    }
  } catch {
    // Not JSON — fall through to text rendering
  }

  if (!parsed) {
    return (
      <pre className="text-xs text-text-secondary font-mono whitespace-pre-wrap p-3 overflow-auto h-full">
        {plan}
      </pre>
    );
  }

  return (
    <div className="overflow-auto h-full p-3 space-y-4">
      {parsed.map((result, i) => (
        <div key={i}>
          <PlanNode node={result.Plan} />
          <div className="mt-2 flex gap-4 text-2xs text-text-muted border-t border-surface-border pt-2">
            {result["Planning Time"] !== undefined && (
              <span>Planning: {result["Planning Time"].toFixed(3)} ms</span>
            )}
            {result["Execution Time"] !== undefined && (
              <span>Execution: {result["Execution Time"].toFixed(3)} ms</span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
