import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Settings as SettingsIcon, Square, Maximize2, X } from "lucide-react";

interface WinBtnProps {
  onClick: () => void;
  label: string;
  danger?: boolean;
  children: React.ReactNode;
}

function WinBtn({ onClick, label, danger, children }: WinBtnProps) {
  const [hover, setHover] = useState(false);
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className="w-9 h-7 grid place-items-center transition-colors duration-100"
      style={{
        background: hover ? (danger ? "var(--cycle)" : "var(--row-hover)") : "transparent",
        color: hover && danger ? "#fff" : "var(--text-2)",
      }}
    >
      {children}
    </button>
  );
}

interface TitlebarProps {
  subtitle?: string;
  onSettings?: () => void;
}

export default function Titlebar({ subtitle, onSettings }: TitlebarProps) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void win.isMaximized().then((m) => {
      if (!cancelled) setMaximized(m);
    });

    void win.onResized(() => {
      void win.isMaximized().then((m) => {
        if (!cancelled) setMaximized(m);
      });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const win = getCurrentWindow();

  return (
    <div
      data-tauri-drag-region
      className="flex items-center justify-between h-9 pl-3 pr-1 shrink-0 select-none"
      style={{ background: "var(--surface-2)", borderBottom: "1px solid var(--border)" }}
    >
      <div className="flex items-center gap-2 min-w-0 pointer-events-none">
        <img src="/icon.png" alt="" className="w-[18px] h-[18px]" />
        <span
          className="text-[11px] font-bold tracking-[0.14em] uppercase"
          style={{ color: "var(--text-2)" }}
        >
          ZAtlas
        </span>
        {subtitle && (
          <span className="mono truncate" style={{ color: "var(--text-3)" }}>
            {subtitle}
          </span>
        )}
      </div>

      <div className="flex items-center" data-tauri-drag-region="false">
        {onSettings && (
          <WinBtn label="Settings" onClick={onSettings}>
            <SettingsIcon size={13} />
          </WinBtn>
        )}
        <WinBtn label="Minimise" onClick={() => void win.minimize()}>
          <Minus size={13} />
        </WinBtn>
        <WinBtn
          label={maximized ? "Restore" : "Maximise"}
          onClick={() => void win.toggleMaximize()}
        >
          {maximized ? <Square size={11} /> : <Maximize2 size={11} />}
        </WinBtn>
        <WinBtn label="Close" danger onClick={() => void win.close()}>
          <X size={13} />
        </WinBtn>
      </div>
    </div>
  );
}
