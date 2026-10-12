import {
  siApachecassandra,
  siClickhouse,
  siCockroachlabs,
  siElasticsearch,
  siMongodb,
  siMysql,
  siPostgresql,
  siRedis,
  siSqlite,
  siSurrealdb,
  type SimpleIcon,
} from "simple-icons";
import { Database } from "lucide-react";
import type { DatabaseType } from "@/lib/types";
import { useAppStore } from "@/stores/useAppStore";
import { cn } from "@/lib/utils";

const LOGOS: Partial<Record<DatabaseType, SimpleIcon>> = {
  postgres: siPostgresql,
  mysql: siMysql,
  sqlite: siSqlite,
  mongodb: siMongodb,
  redis: siRedis,
  cockroachdb: siCockroachlabs,
  clickhouse: siClickhouse,
  cassandra: siApachecassandra,
  elasticsearch: siElasticsearch,
  surrealdb: siSurrealdb,
};

// Microsoft, Oracle and AWS don't permit their logos in Simple Icons; use a lettermark.
const LETTERMARKS: Partial<Record<DatabaseType, { text: string; color: string }>> = {
  mssql: { text: "MS", color: "#CC2927" },
  oracle: { text: "O", color: "#F80000" },
  dynamodb: { text: "DDB", color: "#4053D6" },
};

/** Brand colors too dark to see on the dark theme (e.g. SQLite navy) get lightened. */
function visible(hex: string, dark: boolean): string {
  if (!dark) return `#${hex}`;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  if (luminance > 0.35) return `#${hex}`;
  const lift = (c: number) => Math.round(c + (255 - c) * 0.55);
  return `rgb(${lift(r)}, ${lift(g)}, ${lift(b)})`;
}

/** Database logo sized to the surrounding text (1em). */
export default function DbIcon({ type, className }: { type: DatabaseType; className?: string }) {
  const dark = useAppStore((s) => s.theme) === "dark";
  const logo = LOGOS[type];
  if (logo) {
    return (
      <svg
        role="img"
        aria-label={logo.title}
        viewBox="0 0 24 24"
        className={cn("inline-block w-[1em] h-[1em] flex-shrink-0", className)}
        fill={visible(logo.hex, dark)}
      >
        <path d={logo.path} />
      </svg>
    );
  }
  const mark = LETTERMARKS[type];
  if (mark) {
    return (
      <span
        aria-hidden
        className={cn(
          "inline-flex items-center justify-center w-[1em] h-[1em] flex-shrink-0 rounded-[0.2em] font-bold leading-none text-white",
          className,
        )}
        style={{ backgroundColor: mark.color }}
      >
        <span style={{ fontSize: mark.text.length > 2 ? "0.36em" : "0.5em" }}>{mark.text}</span>
      </span>
    );
  }
  return <Database className={cn("w-[1em] h-[1em] flex-shrink-0", className)} />;
}
