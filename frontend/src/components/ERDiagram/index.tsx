import { useRef, useState, useCallback, useEffect } from "react";
import { RefreshCw, AlertCircle, ZoomIn, ZoomOut, Maximize2 } from "lucide-react";
import { useAppStore } from "@/stores/useAppStore";
import type { ColumnSchema, ForeignKeySchema, SchemaObject } from "@/lib/types";

// ── Types ────────────────────────────────────────────────────────────────────

interface TableNode {
  name: string;
  schema?: string;
  columns: ColumnSchema[];
  foreignKeys: ForeignKeySchema[];
  // Set of column names that are FK columns (computed once from all FK definitions)
  fkColumnNames: Set<string>;
  x: number;
  y: number;
}

// ── Theme colors derived from Zustand (never use CSS variables in SVG) ────────

function useColors() {
  const theme = useAppStore((s) => s.theme);
  const dark = theme === "dark";
  return {
    canvas:       dark ? "#1e1e1e" : "#f0f0f0",
    surface:      dark ? "#252526" : "#ffffff",
    header:       dark ? "#2d2d30" : "#e8e8e8",
    border:       dark ? "#3e3e42" : "#c8c8c8",
    borderSel:    dark ? "#0078d4" : "#0066cc",
    accent:       dark ? "#0078d4" : "#0066cc",
    accentFk:     dark ? "#c586c0" : "#8b008b",  // FK columns — purple
    textPrimary:  dark ? "#cccccc" : "#1e1e1e",
    textSec:      dark ? "#969696" : "#444444",
    textMuted:    dark ? "#6a6a6a" : "#888888",
    relLine:      dark ? "#569cd6" : "#0066cc",   // relation line color
    relOneSide:   dark ? "#4ec9b0" : "#006699",   // one (||) end
    relManySide:  dark ? "#ce9178" : "#994400",   // many (<) end
  };
}

// ── Layout constants ──────────────────────────────────────────────────────────

const TABLE_W = 240;
const HEADER_H = 34;
const ROW_H = 22;
const COL_GAP = 140;  // horizontal gap between tables — must be wide enough for decorations
const ROW_GAP = 80;
const COLS = 4;
const FONT = "Inter, system-ui, sans-serif";
const MONO = "JetBrains Mono, Menlo, monospace";

function tblH(t: Pick<TableNode, "columns">) {
  return HEADER_H + Math.max(t.columns.length, 1) * ROW_H + 8;
}

// Y-coordinate of the CENTER of a column row, in the table's local space
function colLocalY(colIdx: number) {
  return HEADER_H + 4 + colIdx * ROW_H + ROW_H / 2;
}

// Y-coordinate of a column in the SVG global space
function colGlobalY(table: TableNode, colName: string) {
  const idx = Math.max(0, table.columns.findIndex((c) => c.name === colName));
  return table.y + colLocalY(idx);
}

function layoutTables(tables: TableNode[]): TableNode[] {
  const out: TableNode[] = [];
  let y = ROW_GAP;
  for (let i = 0; i < tables.length; i += COLS) {
    const row = tables.slice(i, i + COLS);
    const rowH = Math.max(...row.map(tblH));
    row.forEach((t, col) => out.push({ ...t, x: col * (TABLE_W + COL_GAP), y }));
    y += rowH + ROW_GAP;
  }
  return out;
}

// ── Crow's foot notation helpers ──────────────────────────────────────────────
// dir: "right" = decoration extends in the +x direction from (x, y)
//      "left"  = decoration extends in the -x direction from (x, y)

function CrowFoot({ x, y, dir, color }: { x: number; y: number; dir: "left" | "right"; color: string }) {
  const s = dir === "right" ? 1 : -1;
  const len = 12;
  const fork = 5;
  return (
    <g>
      {/* Center line of fork */}
      <line x1={x} y1={y} x2={x + s * len} y2={y}        stroke={color} strokeWidth={1.5} />
      {/* Upper prong */}
      <line x1={x} y1={y} x2={x + s * len} y2={y - fork} stroke={color} strokeWidth={1.5} />
      {/* Lower prong */}
      <line x1={x} y1={y} x2={x + s * len} y2={y + fork} stroke={color} strokeWidth={1.5} />
      {/* Vertical tick at foot */}
      <line x1={x + s * 3} y1={y - 6} x2={x + s * 3} y2={y + 6} stroke={color} strokeWidth={1.5} />
    </g>
  );
}

function OneBar({ x, y, dir, color }: { x: number; y: number; dir: "left" | "right"; color: string }) {
  const s = dir === "left" ? -1 : 1;
  return (
    <g>
      <line x1={x + s * 4}  y1={y - 6} x2={x + s * 4}  y2={y + 6} stroke={color} strokeWidth={1.5} />
      <line x1={x + s * 9}  y1={y - 6} x2={x + s * 9}  y2={y + 6} stroke={color} strokeWidth={1.5} />
    </g>
  );
}

// ── Single FK relation ────────────────────────────────────────────────────────

interface RelationEdge {
  fkTable: string;
  fkCol: string;
  pkTable: string;
  pkCol: string;
  label: string;
}

function Relation({
  edge,
  tables,
  colors,
}: {
  edge: RelationEdge;
  tables: TableNode[];
  colors: ReturnType<typeof useColors>;
}) {
  const src = tables.find((t) => t.name === edge.fkTable);
  const dst = tables.find((t) => t.name === edge.pkTable);
  if (!src || !dst || src === dst) return null;

  const srcCX = src.x + TABLE_W / 2;
  const dstCX = dst.x + TABLE_W / 2;

  const y1 = colGlobalY(src, edge.fkCol);
  const y2 = colGlobalY(dst, edge.pkCol);

  // Pick which side of each table to connect from
  let x1: number, x2: number, crowDir: "left" | "right", barDir: "left" | "right";

  if (srcCX <= dstCX) {
    // src is left-of-or-equal to dst: exit src right edge, enter dst left edge
    x1 = src.x + TABLE_W;
    x2 = dst.x;
    crowDir = "right";  // crow's foot extends right (into gap)
    barDir  = "left";   // one-bar extends left (into gap)
  } else {
    // src is right of dst: exit src left edge, enter dst right edge
    x1 = src.x;
    x2 = dst.x + TABLE_W;
    crowDir = "left";
    barDir  = "right";
  }

  const span = Math.abs(x2 - x1);
  const bulge = Math.max(50, span * 0.4);
  const cx1 = srcCX <= dstCX ? x1 + bulge : x1 - bulge;
  const cx2 = srcCX <= dstCX ? x2 - bulge : x2 + bulge;

  const line = colors.relLine;
  const many = colors.relManySide;
  const one  = colors.relOneSide;

  return (
    <g>
      {/* Relation line */}
      <path
        d={`M ${x1} ${y1} C ${cx1} ${y1} ${cx2} ${y2} ${x2} ${y2}`}
        fill="none"
        stroke={line}
        strokeWidth={1.5}
        strokeOpacity={0.85}
      />
      {/* Crow's foot at FK/many end (src) */}
      <CrowFoot x={x1} y={y1} dir={crowDir} color={many} />
      {/* One-bar at PK/one end (dst) */}
      <OneBar x={x2} y={y2} dir={barDir} color={one} />
    </g>
  );
}

// ── Table box ─────────────────────────────────────────────────────────────────

function TableBox({
  table,
  selected,
  C,
  onSelect,
  onDragStart,
}: {
  table: TableNode;
  selected: boolean;
  C: ReturnType<typeof useColors>;
  onSelect: () => void;
  onDragStart: (e: React.MouseEvent) => void;
}) {
  const h = tblH(table);
  return (
    <g
      transform={`translate(${table.x},${table.y})`}
      onMouseDown={(e) => { e.stopPropagation(); onSelect(); onDragStart(e); }}
      style={{ cursor: "grab" }}
    >
      {/* Drop shadow */}
      <rect x={3} y={4} width={TABLE_W} height={h} rx={6}
        style={{ fill: "rgba(0,0,0,0.25)", filter: "blur(2px)" }} />

      {/* Body */}
      <rect width={TABLE_W} height={h} rx={6}
        style={{ fill: C.surface, stroke: selected ? C.borderSel : C.border, strokeWidth: selected ? 2 : 1 }} />

      {/* Header */}
      <rect width={TABLE_W} height={HEADER_H} rx={6}
        style={{ fill: C.header }} />
      <rect y={HEADER_H - 6} width={TABLE_W} height={6}
        style={{ fill: C.header }} />

      {/* Table name */}
      <text x={TABLE_W / 2} y={HEADER_H / 2 + 5} textAnchor="middle"
        style={{ fontSize: 12, fontWeight: 700, fill: C.textPrimary, fontFamily: FONT }}>
        {table.schema ? `${table.schema}.${table.name}` : table.name}
      </text>

      {/* Divider line */}
      <line x1={0} y1={HEADER_H} x2={TABLE_W} y2={HEADER_H}
        style={{ stroke: C.border, strokeWidth: 1 }} />

      {/* Column rows */}
      {table.columns.map((col, i) => {
        const rowY = HEADER_H + 4 + i * ROW_H;
        const isPk = col.is_primary_key;
        const isFk = table.fkColumnNames.has(col.name);
        const colColor = isPk ? C.accent : isFk ? C.accentFk : C.textSec;

        return (
          <g key={col.name} transform={`translate(0,${rowY})`}>
            {/* Alternating row background */}
            {i % 2 === 0 && (
              <rect width={TABLE_W} height={ROW_H}
                style={{ fill: C.header, opacity: 0.3 }} />
            )}
            {/* PK / FK colored left indicator */}
            {(isPk || isFk) && (
              <rect width={3} height={ROW_H - 4} x={0} y={2} rx={1}
                style={{ fill: colColor }} />
            )}
            {/* Badge: PK / FK */}
            <text x={8} y={ROW_H / 2 + 4}
              style={{ fontSize: 9, fontWeight: 700, fill: colColor, fontFamily: MONO }}>
              {isPk ? "PK" : isFk ? "FK" : "  "}
            </text>
            {/* Column name */}
            <text x={30} y={ROW_H / 2 + 4}
              style={{ fontSize: 10, fill: colColor, fontFamily: FONT }}>
              {col.name}
            </text>
            {/* Column type */}
            <text x={TABLE_W - 6} y={ROW_H / 2 + 4} textAnchor="end"
              style={{ fontSize: 9, fill: C.textMuted, fontFamily: MONO }}>
              {col.native_type.length > 12 ? col.native_type.slice(0, 12) + "…" : col.native_type}
            </text>
          </g>
        );
      })}
    </g>
  );
}

// ── Main ─────────────────────────────────────────────────────────────────────

export default function ERDiagram({ connectionId }: { connectionId: string }) {
  const { schemas, loadSchema, activeConnections } = useAppStore();
  const schema = schemas[connectionId];
  const conn = activeConnections.find((c) => c.id === connectionId);
  const C = useColors();

  const svgRef = useRef<SVGSVGElement>(null);
  const [vb, setVb] = useState({ x: -30, y: -30, w: 1600, h: 1000 });
  const [selected, setSelected] = useState<string | null>(null);
  const [tables, setTables] = useState<TableNode[]>([]);
  const [edges, setEdges] = useState<RelationEdge[]>([]);

  useEffect(() => {
    if (!schema || schema === "loading" || "error" in schema) return;

    const rawTables = schema.objects.filter((o: SchemaObject) => o.kind === "table");

    const nodes: Omit<TableNode, "fkColumnNames" | "x" | "y">[] = rawTables.map((o: SchemaObject) => ({
      name: (o.name as string) ?? "",
      schema: (o as { schema?: string }).schema,
      columns: ((o as { columns?: ColumnSchema[] }).columns ?? []),
      foreignKeys: ((o as { foreign_keys?: ForeignKeySchema[] }).foreign_keys ?? []),
    }));

    // Build FK column sets
    const withFkSets: Omit<TableNode, "x" | "y">[] = nodes.map((n) => ({
      ...n,
      fkColumnNames: new Set(n.foreignKeys.flatMap((fk) => fk.columns)),
    }));

    // Build edge list
    const newEdges: RelationEdge[] = [];
    for (const tbl of withFkSets) {
      for (const fk of tbl.foreignKeys) {
        fk.columns.forEach((fkCol, idx) => {
          newEdges.push({
            fkTable: tbl.name,
            fkCol,
            pkTable: fk.referenced_table,
            pkCol: fk.referenced_columns[idx] ?? fk.referenced_columns[0] ?? "",
            label: fk.name,
          });
        });
      }
    }

    setEdges(newEdges);
    setTables(layoutTables(withFkSets.map((t) => ({ ...t, x: 0, y: 0 }))));
  }, [schema]);

  if (schema === undefined) loadSchema(connectionId);

  // ── Interaction ───────────────────────────────────────────────────────────

  const isPanning = useRef(false);
  const panStart = useRef({ mx: 0, my: 0, vx: 0, vy: 0 });

  const dragTable = useRef<string | null>(null);
  const dragOff = useRef({ x: 0, y: 0 });

  const svgPoint = useCallback((e: React.MouseEvent) => {
    if (!svgRef.current) return { x: 0, y: 0 };
    const r = svgRef.current.getBoundingClientRect();
    return {
      x: (e.clientX - r.left) / r.width * vb.w + vb.x,
      y: (e.clientY - r.top) / r.height * vb.h + vb.y,
    };
  }, [vb]);

  const onMouseDown = useCallback((e: React.MouseEvent<SVGSVGElement>) => {
    if (e.button !== 0) return;
    isPanning.current = true;
    panStart.current = { mx: e.clientX, my: e.clientY, vx: vb.x, vy: vb.y };
    setSelected(null);
  }, [vb]);

  const onTableDragStart = useCallback((name: string, e: React.MouseEvent) => {
    dragTable.current = name;
    const pt = svgPoint(e);
    const t = tables.find((t) => t.name === name);
    if (t) dragOff.current = { x: pt.x - t.x, y: pt.y - t.y };
  }, [tables, svgPoint]);

  const onMouseMove = useCallback((e: React.MouseEvent<SVGSVGElement>) => {
    if (dragTable.current) {
      const pt = svgPoint(e);
      setTables((ts) => ts.map((t) =>
        t.name === dragTable.current
          ? { ...t, x: pt.x - dragOff.current.x, y: pt.y - dragOff.current.y }
          : t,
      ));
      return;
    }
    if (!isPanning.current || !svgRef.current) return;
    const r = svgRef.current.getBoundingClientRect();
    const sx = vb.w / r.width;
    const sy = vb.h / r.height;
    setVb((v) => ({
      ...v,
      x: panStart.current.vx - (e.clientX - panStart.current.mx) * sx,
      y: panStart.current.vy - (e.clientY - panStart.current.my) * sy,
    }));
  }, [svgPoint, vb]);

  const onMouseUp = useCallback(() => {
    isPanning.current = false;
    dragTable.current = null;
  }, []);

  const onWheel = useCallback((e: React.WheelEvent<SVGSVGElement>) => {
    e.preventDefault();
    const f = e.deltaY > 0 ? 1.12 : 0.89;
    setVb((v) => ({
      ...v,
      w: Math.max(400, Math.min(6000, v.w * f)),
      h: Math.max(300, Math.min(5000, v.h * f)),
    }));
  }, []);

  const resetLayout = () => {
    setVb({ x: -30, y: -30, w: 1600, h: 1000 });
    setTables((ts) => layoutTables(ts.map((t) => ({ ...t, x: 0, y: 0 }))));
  };

  // ── States ────────────────────────────────────────────────────────────────

  if (schema === "loading" || schema === undefined) {
    return (
      <div className="flex items-center justify-center h-full text-text-muted text-xs gap-2">
        <RefreshCw size={14} className="animate-spin" /> Loading schema…
      </div>
    );
  }
  if ("error" in schema) {
    return (
      <div className="flex items-center justify-center h-full text-red-400 text-xs gap-2">
        <AlertCircle size={14} /> {schema.error}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full" style={{ background: C.canvas }}>
      {/* Toolbar */}
      <div className="flex items-center gap-3 px-3 py-1.5 border-b border-surface-border bg-surface-raised flex-shrink-0">
        <span className="text-xs text-text-secondary font-medium">
          {conn?.host ?? "ER Diagram"} — {tables.length} tables · {edges.length} relations
        </span>
        {/* Legend */}
        <div className="flex items-center gap-3 ml-3 text-2xs text-text-muted">
          <span style={{ color: C.relManySide }} className="font-mono font-bold">{"<|"}</span>
          <span>many (FK)</span>
          <span style={{ color: C.relOneSide }} className="font-mono font-bold">{"||"}</span>
          <span>one (PK)</span>
          <span style={{ color: C.accent }} className="font-mono font-bold">PK</span>
          <span style={{ color: C.accentFk }} className="font-mono font-bold">FK</span>
        </div>
        <div className="flex-1" />
        <button className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
          onClick={() => setVb((v) => ({ ...v, w: v.w * 0.85, h: v.h * 0.85 }))} title="Zoom in">
          <ZoomIn size={13} />
        </button>
        <button className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
          onClick={() => setVb((v) => ({ ...v, w: v.w * 1.15, h: v.h * 1.15 }))} title="Zoom out">
          <ZoomOut size={13} />
        </button>
        <button className="p-1 text-text-muted hover:text-text-primary hover:bg-surface-overlay rounded transition-colors"
          onClick={resetLayout} title="Reset layout">
          <Maximize2 size={13} />
        </button>
      </div>

      {/* Canvas */}
      <div className="flex-1 overflow-hidden">
        <svg
          ref={svgRef}
          width="100%" height="100%"
          viewBox={`${vb.x} ${vb.y} ${vb.w} ${vb.h}`}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={onMouseUp}
          onMouseLeave={onMouseUp}
          onWheel={onWheel}
          style={{ display: "block", userSelect: "none" }}
        >
          {/* Grid background dots */}
          <defs>
            <pattern id="grid" width={40} height={40} patternUnits="userSpaceOnUse">
              <circle cx={20} cy={20} r={1}
                style={{ fill: C.border, opacity: 0.4 }} />
            </pattern>
          </defs>
          <rect x={vb.x} y={vb.y} width={vb.w} height={vb.h} fill="url(#grid)" />

          {/* ── Layer 1: Table boxes ── */}
          {tables.map((t) => (
            <TableBox
              key={t.name}
              table={t}
              selected={selected === t.name}
              C={C}
              onSelect={() => setSelected(t.name)}
              onDragStart={(e) => onTableDragStart(t.name, e)}
            />
          ))}

          {/* ── Layer 2: Relations (on top of tables) ── */}
          {edges.map((e) => (
            <Relation key={`${e.fkTable}-${e.label}`} edge={e} tables={tables} colors={C} />
          ))}

          {tables.length === 0 && (
            <text x={40} y={60}
              style={{ fontSize: 14, fill: C.textMuted, fontFamily: FONT }}>
              No tables found in this schema.
            </text>
          )}
          {tables.length > 0 && edges.length === 0 && (
            <text x={40} y={60}
              style={{ fontSize: 12, fill: C.textMuted, fontFamily: FONT }}>
              No foreign key relationships found. FK data depends on the driver&apos;s schema introspection.
            </text>
          )}
        </svg>
      </div>
    </div>
  );
}
