import { CheckCircle2, XCircle, Info, X } from "lucide-react";
import { useAppStore, type Toast } from "@/stores/useAppStore";
import { cn } from "@/lib/utils";

const ICONS: Record<Toast["type"], React.ReactNode> = {
  success: <CheckCircle2 size={13} className="text-green-400 flex-shrink-0" />,
  error: <XCircle size={13} className="text-red-400 flex-shrink-0" />,
  info: <Info size={13} className="text-accent flex-shrink-0" />,
};

const BAR_COLOR: Record<Toast["type"], string> = {
  success: "bg-green-400",
  error: "bg-red-400",
  info: "bg-accent",
};

export default function ToastContainer() {
  const { toasts, removeToast } = useAppStore();
  if (!toasts.length) return null;

  return (
    <div className="fixed bottom-7 right-4 z-[300] flex flex-col gap-2 pointer-events-none">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn(
            "pointer-events-auto flex items-start gap-2.5 min-w-[240px] max-w-[360px]",
            "bg-surface-raised border border-surface-border rounded-lg shadow-2xl",
            "px-3 py-2.5 animate-in slide-in-from-right-4 fade-in duration-200",
          )}
        >
          {ICONS[t.type]}
          <span className="text-xs text-text-primary flex-1 leading-relaxed">{t.message}</span>
          <button
            className="text-text-muted hover:text-text-primary transition-colors flex-shrink-0 mt-px"
            onClick={() => removeToast(t.id)}
          >
            <X size={11} />
          </button>
          {/* Progress bar that shrinks over 3.5s */}
          <div className="absolute bottom-0 left-0 right-0 h-0.5 rounded-b-lg overflow-hidden">
            <div
              className={cn("h-full", BAR_COLOR[t.type], "animate-shrink")}
            />
          </div>
        </div>
      ))}
    </div>
  );
}
