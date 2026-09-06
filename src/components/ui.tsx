import { useId, useRef, useState, type ReactNode } from "react";
import { Check, ChevronRight } from "lucide-react";


export function RailLabel({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-center gap-2 px-4 pt-5 pb-2">
      <span
        className="text-[10px] font-semibold tracking-[0.16em] uppercase shrink-0"
        style={{ color: "var(--text-3)" }}
      >
        {children}
      </span>
      <span
        aria-hidden
        className="flex-1 h-px"
        style={{ background: "var(--border)" }}
      />
    </div>
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  count,
  hint,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  count?: number;
  hint?: string;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      title={hint}
      className="group flex items-center gap-2.5 w-full px-4 h-7 text-left text-[12px] transition-colors duration-100 hover:bg-[var(--row-hover)]"
    >
      <span
        aria-hidden
        className="grid place-items-center w-[14px] h-[14px] rounded-[2px] shrink-0 transition-colors duration-100"
        style={{
          background: checked ? "var(--accent-soft)" : "transparent",
          border: `1px solid ${checked ? "var(--accent)" : "var(--border-strong)"}`,
          color: "var(--accent)",
        }}
      >
        {checked && <Check size={10} strokeWidth={3.5} />}
      </span>
      <span
        className="truncate transition-colors duration-100"
        style={{ color: checked ? "var(--text)" : "var(--text-2)" }}
      >
        {label}
      </span>
      {count !== undefined && (
        <span className="mono ml-auto shrink-0" style={{ color: "var(--text-3)" }}>
          {count}
        </span>
      )}
    </button>
  );
}


export function Slider({
  value,
  min,
  max,
  step = 1,
  onChange,
  label,
  format,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  label: string;
  format?: (v: number) => string;
}) {
  const id = useId();
  const pct = max === min ? 0 : ((value - min) / (max - min)) * 100;

  return (
    <div className="px-4 pt-1.5 pb-2">
      <div className="flex items-baseline justify-between mb-1.5">
        <label htmlFor={id} className="text-[11px]" style={{ color: "var(--text-3)" }}>
          {label}
        </label>
        <span className="mono text-[11px]" style={{ color: "var(--text-2)" }}>
          {format ? format(value) : value}
        </span>
      </div>


      <div className="relative h-4 flex items-center mx-[2px]">
        <div
          aria-hidden
          className="absolute inset-x-0 h-[3px] rounded-[2px]"
          style={{ background: "var(--control)", border: "1px solid var(--border)" }}
        />
        <div
          aria-hidden
          className="absolute left-0 h-[3px] rounded-[2px]"
          style={{ width: `${pct}%`, background: "var(--accent)" }}
        />


        <div
          aria-hidden
          className="absolute w-[3px] h-3.5 rounded-[1px] -translate-x-1/2 pointer-events-none"
          style={{
            left: `${pct}%`,
            background: "var(--accent)",
            boxShadow: "0 0 0 2px var(--surface)",
          }}
        />
        <input
          id={id}
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="absolute inset-0 w-full opacity-0 cursor-pointer"
          style={{ margin: 0 }}
        />
      </div>
    </div>
  );
}


export function TextInput({
  value,
  onChange,
  placeholder,
  mono = true,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  mono?: boolean;
}) {
  const [focused, setFocused] = useState(false);
  return (
    <input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      placeholder={placeholder}
      className={`${mono ? "mono" : "text-[12px]"} w-full h-7 px-2.5 rounded-md outline-none transition-colors duration-100`}
      style={{
        background: "var(--input)",
        border: `1px solid ${focused ? "var(--accent)" : "var(--border)"}`,
        color: "var(--text)",
      }}
    />
  );
}


export function Disclosure({
  title,
  children,
  defaultOpen = false,
  tone = "quiet",
}: {
  title: ReactNode;
  children: ReactNode;
  defaultOpen?: boolean;
  tone?: "quiet" | "loud";
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div style={{ borderBottom: "1px solid var(--border)" }}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 w-full px-4 h-8 text-left text-[11px] transition-colors duration-100 hover:bg-[var(--row-hover)]"
        style={{ color: tone === "loud" ? "var(--text-2)" : "var(--text-3)" }}
      >
        <ChevronRight
          size={12}
          className="shrink-0 transition-transform duration-150"
          style={{ transform: open ? "rotate(90deg)" : "none" }}
        />
        <span className="truncate">{title}</span>
      </button>
      {open && <div className="pb-1">{children}</div>}
    </div>
  );
}


export function Stat({
  label,
  value,
  warn,
  title,
  fill,
}: {
  label: string;
  value: ReactNode;
  warn?: boolean;
  title?: string;

  fill?: number;
}) {
  return (
    <div className="relative px-4 h-[23px] flex items-center" title={title}>
      {fill !== undefined && (


        <span
          aria-hidden
          className="absolute left-0 top-[3px] bottom-[3px] transition-[width] duration-300"
          style={{
            width: `${Math.max(1.5, Math.min(100, fill * 100))}%`,
            background:
              "linear-gradient(to right, color-mix(in srgb, var(--accent) 16%, transparent), transparent)",
            borderLeft: "2px solid var(--accent)",
            opacity: 0.55,
          }}
        />
      )}
      <div className="relative flex items-baseline justify-between gap-2 w-full">
        <span className="text-[11px] truncate" style={{ color: "var(--text-3)" }}>
          {label}
        </span>
        <span
          className="mono shrink-0"
          style={{ color: warn ? "var(--warn)" : "var(--text)" }}
        >
          {value}
        </span>
      </div>
    </div>
  );
}


export function Button({
  children,
  onClick,
  variant = "default",
  disabled,
  title,
  square,
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "default" | "accent" | "ghost";
  disabled?: boolean;
  title?: string;

  square?: boolean;
}) {
  const [hover, setHover] = useState(false);

  const styles: Record<string, React.CSSProperties> = {
    default: {
      background: hover ? "var(--control-hover)" : "var(--control)",
      color: "var(--text)",
      borderColor: hover ? "var(--border-strong)" : "var(--border)",
    },
    accent: {
      background: "var(--accent-soft)",
      color: "var(--accent)",
      borderColor: "var(--accent)",
    },
    ghost: {
      background: hover ? "var(--row-hover)" : "transparent",
      color: hover ? "var(--text)" : "var(--text-2)",
      borderColor: "transparent",
    },
  };

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={square && typeof title === "string" ? title : undefined}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className={`${square ? "w-7 grid place-items-center" : "px-2.5"} relative h-7 shrink-0 rounded-[3px] border text-[12px] transition-colors duration-100 disabled:opacity-40 disabled:cursor-not-allowed`}
      style={styles[variant]}
    >
      {children}
      {variant === "accent" && <CornerTicks />}
    </button>
  );
}


export function CornerTicks({ inset = 0 }: { inset?: number }) {
  const arm = 5;
  const common: React.CSSProperties = {
    position: "absolute",
    width: arm,
    height: arm,
    borderColor: "var(--tick)",
    pointerEvents: "none",
  };
  return (
    <>
      <span
        aria-hidden
        style={{ ...common, top: inset, left: inset, borderTop: "1px solid", borderLeft: "1px solid" }}
      />
      <span
        aria-hidden
        style={{ ...common, top: inset, right: inset, borderTop: "1px solid", borderRight: "1px solid" }}
      />
      <span
        aria-hidden
        style={{ ...common, bottom: inset, left: inset, borderBottom: "1px solid", borderLeft: "1px solid" }}
      />
      <span
        aria-hidden
        style={{ ...common, bottom: inset, right: inset, borderBottom: "1px solid", borderRight: "1px solid" }}
      />
    </>
  );
}


export function Empty({
  title,
  hint,
  action,
}: {
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="h-full min-h-[120px] grid place-items-center px-6 py-8">
      <div className="text-center max-w-[380px]">
        <p style={{ color: "var(--text-2)" }}>{title}</p>
        {hint && (
          <p className="mt-2 text-[12px] leading-relaxed" style={{ color: "var(--text-3)" }}>
            {hint}
          </p>
        )}
        {action && <div className="mt-4 flex justify-center">{action}</div>}
      </div>
    </div>
  );
}

export function Spinner({ label }: { label?: string }) {
  return (
    <div className="flex items-center gap-2 text-[12px]" style={{ color: "var(--text-3)" }}>
      <span
        className="inline-block w-3 h-3 rounded-full border-[1.5px] animate-spin shrink-0"
        style={{
          borderColor: "var(--border-strong)",
          borderTopColor: "var(--accent)",
        }}
      />
      {label}
    </div>
  );
}


export function Divider() {
  return (
    <span
      aria-hidden
      className="w-px h-4 shrink-0 mx-0.5"
      style={{ background: "var(--border)" }}
    />
  );
}


export function compact(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}


export function ago(seconds: number): string {
  if (!seconds) return "—";
  const delta = Date.now() / 1000 - seconds;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  const days = Math.floor(delta / 86400);
  if (days < 30) return `${days}d ago`;
  if (days < 365) return `${Math.floor(days / 30)}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
}


export function ellipsisMiddle(text: string, max: number): string {
  if (text.length <= max) return text;
  const keepEnd = Math.max(8, Math.floor(max * 0.6));
  const keepStart = Math.max(0, max - keepEnd - 1);
  return `${text.slice(0, keepStart)}…${text.slice(text.length - keepEnd)}`;
}


export function useAutoFocus<T extends HTMLElement>() {
  return useRef<T | null>(null);
}
