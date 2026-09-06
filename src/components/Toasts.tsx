import { X } from "lucide-react";

import { useToastStore } from "../store/useToastStore";

const COLOR: Record<string, string> = {
  info: "var(--text-2)",
  success: "var(--ok)",
  warn: "var(--warn)",
  error: "var(--cycle)",
};

export default function Toasts() {
  const { toasts, dismiss } = useToastStore();
  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-3 right-3 z-50 flex flex-col gap-2 max-w-[360px]">
      {toasts.map((t) => (
        <div
          key={t.id}
          className="flex items-start gap-2 px-3 py-2 rounded-md"
          style={{
            background: "var(--raised)",
            border: `1px solid ${COLOR[t.level]}`,
            boxShadow: "var(--shadow)",
          }}
        >
          <div className="min-w-0">
            <div className="text-[12px]" style={{ color: COLOR[t.level] }}>
              {t.message}
            </div>
            {t.detail && (
              <div className="text-[11px] mt-0.5" style={{ color: "var(--text-3)" }}>
                {t.detail}
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={() => dismiss(t.id)}
            className="ml-auto shrink-0"
            style={{ color: "var(--text-3)" }}
            aria-label="Dismiss"
          >
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
