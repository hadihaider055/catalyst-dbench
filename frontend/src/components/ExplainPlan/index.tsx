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
  "Shared Hit Blocks"?: number;
  "Shared Read Blocks"?: number;
  "Plans"?: PgPlanNode[];
  [key: string]: unknown;
}

interface PgExplainResult {
  Plan: PgPlanNode;
  "Planning Time"?: number;
  "Execution Time"?: number;
}

// ── Traverse all nodes to compute max values ──────────────────────────────────

function collectAllNodes(node: PgPlanNode): PgPlanNode[] {
  const children = (node.Plans ?? []).flatMap(collectAllNodes);
  return [node, ...children];
}

// ── Plan node tree ────────────────────────────────────────────────────────────

function PlanNode({
  node,
  depth = 0,
  maxTime,
  maxCost,
}: {
  node: PgPlanNode;
  depth?: number;
  maxTime: number;
  maxCost: number;
}) {
  const [open, setOpen] = useState(true);
  const hasChildren = node.Plans && node.Plans.length > 0;

  const totalCost = node["Total Cost"] ?? 0;
  const actualTime = node["Actual Total Time"];

  const costClass =
    totalCost > 1000
      ? "text-red-400"
      : totalCost > 100
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

  const timePct = maxTime > 0 && actualTime !== undefined ? (actualTime / maxTime) * 100 : 0;
  const costPct = maxCost > 0 ? (totalCost / maxCost) * 100 : 0;

  const sharedHit = node["Shared Hit Blocks"];
  const sharedRead = node["Shared Read Blocks"];
  const totalBlocks = (sharedHit ?? 0) + (sharedRead ?? 0);
  const hitPct = totalBlocks > 0 ? ((sharedHit ?? 0) / totalBlocks) * 100 : null;

  const planRows = node["Plan Rows"];
  const actualRows = node["Actual Rows"];
  const rowEstimateOff =
    planRows !== undefined && actualRows !== undefined && planRows > 0
      ? actualRows / planRows
      : null;

  return (
    <div style={{ paddingLeft: depth > 0 ? 16 : 0 }} className="text-xs">
      <div
        className="flex items-start gap-1.5 py-0.5 px-1 rounded hover:bg-surface-overlay cursor-default"
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

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-medium text-text-primary">{label}</span>

            <span className={cn("tabular-nums flex-shrink-0", costClass)}>
              {totalCost > 0 && `cost=${node["Startup Cost"]?.toFixed(2)}..${totalCost.toFixed(2)}`}
            </span>

            {actualTime !== undefined && (
              <span className="text-text-muted tabular-nums flex-shrink-0">
                {actualTime.toFixed(3)} ms
              </span>
            )}

            {planRows !== undefined && (
              <span
                className={cn(
                  "tabular-nums flex-shrink-0",
                  rowEstimateOff !== null && rowEstimateOff > 10
                    ? "text-red-400"
                    : rowEstimateOff !== null && rowEstimateOff > 2
                      ? "text-yellow-400"
                      : "text-text-muted",
                )}
                title={rowEstimateOff !== null ? `Estimate accuracy: ${rowEstimateOff.toFixed(1)}x` : undefined}
              >
                rows={actualRows !== undefined ? `${actualRows}/${planRows}` : planRows}
              </span>
            )}
          </div>

          {/* Timing bar */}
          {actualTime !== undefined && timePct > 0 && (
            <div className="mt-0.5 flex items-center gap-1.5">
              <div className="flex-1 h-1.5 bg-surface-overlay rounded-full overflow-hidden max-w-48">
                <div
                  className={cn(
                    "h-full rounded-full transition-all",
                    timePct > 80 ? "bg-red-500" : timePct > 50 ? "bg-yellow-500" : "bg-green-500",
                  )}
                  style={{ width: `${timePct}%` }}
                />
              </div>
              <span className="text-2xs text-text-muted tabular-nums">{timePct.toFixed(1)}%</span>
            </div>
          )}

          {/* Cost bar (when no timing available) */}
          {actualTime === undefined && costPct > 0 && (
            <div className="mt-0.5 flex items-center gap-1.5">
              <div className="flex-1 h-1.5 bg-surface-overlay rounded-full overflow-hidden max-w-48">
                <div
                  className={cn(
                    "h-full rounded-full",
                    costPct > 80 ? "bg-red-500/70" : costPct > 50 ? "bg-yellow-500/70" : "bg-green-500/70",
                  )}
                  style={{ width: `${costPct}%` }}
                />
              </div>
            </div>
          )}

          {/* Buffer stats */}
          {hitPct !== null && (
            <div className="mt-0.5 flex items-center gap-1.5">
              <div className="flex-1 h-1 bg-surface-overlay rounded-full overflow-hidden max-w-48">
                <div
                  className="h-full bg-accent/60 rounded-full"
                  style={{ width: `${hitPct}%` }}
                  title={`Cache hit: ${hitPct.toFixed(0)}%`}
                />
              </div>
              <span className="text-2xs text-text-muted tabular-nums">
                {sharedHit ?? 0} hit / {sharedRead ?? 0} read
              </span>
            </div>
          )}
        </div>
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
        <PlanNode key={i} node={child} depth={depth + 1} maxTime={maxTime} maxCost={maxCost} />
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
      {parsed.map((result, i) => {
        const allNodes = collectAllNodes(result.Plan);
        const maxTime = Math.max(
          0,
          ...allNodes.map((n) => n["Actual Total Time"] ?? 0),
        );
        const maxCost = Math.max(
          0,
          ...allNodes.map((n) => n["Total Cost"] ?? 0),
        );

        return (
          <div key={i}>
            <PlanNode node={result.Plan} maxTime={maxTime} maxCost={maxCost} />
            <div className="mt-2 flex gap-4 text-2xs text-text-muted border-t border-surface-border pt-2">
              {result["Planning Time"] !== undefined && (
                <span>Planning: {result["Planning Time"].toFixed(3)} ms</span>
              )}
              {result["Execution Time"] !== undefined && (
                <span>Execution: {result["Execution Time"].toFixed(3)} ms</span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
