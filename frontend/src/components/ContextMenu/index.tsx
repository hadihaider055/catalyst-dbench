import { useEffect, useRef } from "react";

// Utils
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/useAppStore";

export interface ContextMenuItem {
  label: string;
  icon?: React.ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
}

export interface ContextMenuSeparator {
  separator: true;
}

export type ContextMenuEntry = ContextMenuItem | ContextMenuSeparator;

interface Props {
  x: number;
  y: number;
  items: ContextMenuEntry[];
  onClose: () => void;
}

export default function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const zoom = useAppStore((s) => s.zoom);

  useEffect(() => {
    const close = () => onClose();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    // Slight delay so the right-click that opened this doesn't immediately close it
    const t = setTimeout(() => {
      document.addEventListener("mousedown", close);
      document.addEventListener("keydown", key);
    }, 10);
    return () => {
      clearTimeout(t);
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);

  // Keep menu in viewport. CSS zoom on body puts `position: fixed` in zoomed
  // coordinate space, so we divide by zoom so the menu lands at the cursor.
  const menuH = items.length * 28 + 8;
  const rawTop = y + menuH > window.innerHeight * zoom ? y - menuH : y;
  const rawLeft = x + 200 > window.innerWidth * zoom ? x - 200 : x;
  const top = rawTop / zoom;
  const left = rawLeft / zoom;

  return (
    <div
      ref={ref}
      className="fixed z-[100] bg-surface-raised border border-surface-border rounded shadow-2xl min-w-48 py-1 text-xs"
      style={{ left, top }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {items.map((item, i) => {
        if ("separator" in item) {
          return (
            <div key={i} className="my-1 border-t border-surface-border" />
          );
        }
        return (
          <button
            key={i}
            className={cn(
              "flex items-center gap-2 w-full px-3 py-1.5 text-left transition-colors",
              item.disabled
                ? "text-text-muted cursor-not-allowed opacity-50"
                : item.danger
                  ? "text-red-400 hover:bg-red-950/30"
                  : "text-text-secondary hover:bg-surface-overlay hover:text-text-primary",
            )}
            onClick={() => {
              if (!item.disabled) {
                item.onClick();
                onClose();
              }
            }}
          >
            {item.icon && (
              <span className="flex-shrink-0 text-text-muted">{item.icon}</span>
            )}
            {item.label}
          </button>
        );
      })}
    </div>
  );
}
