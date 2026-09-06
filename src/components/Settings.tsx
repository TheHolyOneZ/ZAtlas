import { useEffect, useState } from "react";
import { ArrowUpRight, Check, Code, Gamepad2, Globe, Map, User, X } from "lucide-react";

import { Checkbox, CornerTicks } from "./ui";
import { severityCss } from "../lib/severity";
import { SHORTCUTS, shortcutLabel } from "../lib/shortcuts";
import { api, type LspServerDto } from "../lib/tauri";
import { openUrl } from "@tauri-apps/plugin-opener";
import { type LspChoice, THEMES, useSettingsStore } from "../store/useSettingsStore";


const LINKS: {
  href: string;
  label: string;
  hint: string;
  icon: typeof Globe;
}[] = [
  {
    href: "https://zsync.eu/zatlas/",
    label: "ZAtlas",
    hint: "Downloads, and an overview of what it does",
    icon: Map,
  },
  {
    href: "https://github.com/TheHolyOneZ/ZAtlas",
    label: "Source on GitHub",
    hint: "The code, and the full documentation. Builds live on the site.",
    icon: Code,
  },
  {
    href: "https://zsync.eu/",
    label: "ZSync",
    hint: "The rest of the Z tools",
    icon: Globe,
  },
  {
    href: "https://zlogic.eu",
    label: "ZLogic",
    hint: "Game modifications and modding",
    icon: Gamepad2,
  },
  {
    href: "https://github.com/TheHolyOneZ",
    label: "TheHolyOneZ",
    hint: "The author",
    icon: User,
  },
];


function LinkRow({ link }: { link: (typeof LINKS)[number] }) {
  const Icon = link.icon;
  return (
    <button
      type="button"
      onClick={() => void openUrl(link.href).catch(() => {})}
      title={link.href}
      className="group/link flex items-center gap-2.5 w-full px-2 py-1.5 rounded-[3px] text-left transition-colors duration-100 hover:bg-[var(--row-hover)]"
    >
      <Icon size={13} className="shrink-0" style={{ color: "var(--text-3)" }} />
      <span className="min-w-0">
        <span className="block text-[12px]" style={{ color: "var(--text)" }}>
          {link.label}
        </span>
        <span className="block text-[11px] truncate" style={{ color: "var(--text-3)" }}>
          {link.hint}
        </span>
      </span>
      <ArrowUpRight
        size={12}
        className="ml-auto shrink-0 opacity-0 group-hover/link:opacity-100 transition-opacity duration-100"
        style={{ color: "var(--accent)" }}
      />
    </button>
  );
}


const LSP_MODES: { id: LspChoice; label: string; blurb: string }[] = [
  {
    id: null,
    label: "Follow zatlas.toml",
    blurb: "Whatever the repository asks for. Off, unless it says otherwise.",
  },
  {
    id: "off",
    label: "Off",
    blurb: "Nothing is started. The heuristic resolvers do all the work.",
  },
  {
    id: "repair",
    label: "Repair",
    blurb:
      "Ask a language server only about imports the resolvers could not place. Costs nothing when there are none.",
  },
  {
    id: "verify",
    label: "Verify",
    blurb:
      "Ask about every import and report disagreements as findings. Slow, and the mode that audits the map itself.",
  },
];


function ModeRow({
  mode,
  on,
  onPick,
}: {
  mode: (typeof LSP_MODES)[number];
  on: boolean;
  onPick: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={on}
      onClick={onPick}
      className="relative flex flex-col items-start w-full pl-3.5 pr-3 py-2 text-left transition-colors duration-100"
      style={{
        background: on ? "var(--accent-soft)" : "transparent",
        borderBottom: "1px solid var(--border)",
      }}
    >
      <span
        aria-hidden
        className="absolute left-0 top-0 bottom-0 w-[3px]"
        style={{ background: on ? "var(--accent)" : "transparent" }}
      />
      <span
        className="text-[12px]"
        style={{ color: on ? "var(--accent)" : "var(--text-2)" }}
      >
        {mode.label}
      </span>
      <span className="text-[11px] leading-snug mt-0.5" style={{ color: "var(--text-3)" }}>
        {mode.blurb}
      </span>
      {on && <CornerTicks inset={3} />}
    </button>
  );
}


export default function Settings({ onClose }: { onClose: () => void }) {
  const { theme, setTheme, contours, setContours, lspMode, setLspMode } =
    useSettingsStore();
  const [servers, setServers] = useState<LspServerDto[]>([]);

  useEffect(() => {
    let live = true;
    api
      .lspServers()
      .then((list) => {
        if (live) setServers(list);
      })


      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const groups = Array.from(new Set(SHORTCUTS.map((s) => s.group)));

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={onClose}
    >
      <div
        className="w-[680px] max-w-[92vw] max-h-[82vh] flex flex-col rounded-lg overflow-hidden"
        style={{
          background: "var(--raised)",
          border: "1px solid var(--border-strong)",
          boxShadow: "var(--shadow)",
          backdropFilter: "blur(24px) saturate(140%)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="flex items-center px-3 h-10 shrink-0"
          style={{ borderBottom: "1px solid var(--border)" }}
        >
          <span className="text-[12px]" style={{ color: "var(--text)" }}>
            Settings
          </span>
          <button
            type="button"
            onClick={onClose}
            className="ml-auto"
            style={{ color: "var(--text-3)" }}
            aria-label="Close"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4">
          <div
            className="text-[10px] font-semibold tracking-[0.12em] uppercase mb-2"
            style={{ color: "var(--text-3)" }}
          >
            Theme
          </div>
          <div className="grid grid-cols-2 gap-2">
            {THEMES.map((t) => {
              const on = theme === t.id;
              return (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => setTheme(t.id)}
                  className="flex items-center gap-2 px-3 h-9 rounded-md text-left"
                  style={{
                    background: on ? "var(--accent-soft)" : "var(--surface-2)",
                    border: `1px solid ${on ? "var(--accent)" : "var(--border)"}`,
                    color: on ? "var(--accent)" : "var(--text-2)",
                  }}
                >


                  <span className="flex shrink-0">
                    {[0, 0.35, 0.7, 1].map((h) => (
                      <span
                        key={h}
                        className="w-2 h-4 first:rounded-l last:rounded-r"
                        style={{
                          background: severityCss(h, t.id === "topographic-light"),
                        }}
                      />
                    ))}
                  </span>
                  <span className="text-[12px]">{t.label}</span>
                  {on && <Check size={13} className="ml-auto" />}
                </button>
              );
            })}
          </div>

          <div
            className="text-[10px] font-semibold tracking-[0.12em] uppercase mt-6 mb-2"
            style={{ color: "var(--text-3)" }}
          >
            Canvas
          </div>
          <div className="-mx-4">
            <Checkbox
              checked={contours}
              onChange={setContours}
              label="Contour ground"
              hint="A faint elevation texture behind the graph. Drawn in world space, so it moves with the map."
            />
          </div>

          <div
            className="text-[10px] font-semibold tracking-[0.12em] uppercase mt-6 mb-2"
            style={{ color: "var(--text-3)" }}
          >
            Language servers
          </div>
          <p className="text-[11px] leading-relaxed mb-2" style={{ color: "var(--text-3)" }}>
            A second opinion on the map. The heuristic resolvers stay in charge —
            they are deterministic and fast — and a language server is asked only
            where it earns the wait.
          </p>
          <div
            className="rounded-md overflow-hidden"
            role="radiogroup"
            aria-label="Language server mode"
            style={{ border: "1px solid var(--border)" }}
          >
            {LSP_MODES.map((m) => (
              <ModeRow
                key={m.label}
                mode={m}
                on={lspMode === m.id}
                onPick={() => {
                  setLspMode(m.id);


                  void api.setLspMode(m.id).catch(() => {});
                }}
              />
            ))}
          </div>

          <div className="mt-3">
            {servers.length === 0 ? (
              <p className="text-[11px]" style={{ color: "var(--text-3)" }}>
                Scan a repository to see which servers apply to it.
              </p>
            ) : (
              servers.map((s) => (
                <div key={s.id} className="flex items-baseline gap-2.5 py-0.5">


                  <span
                    aria-hidden
                    className="w-1.5 h-1.5 rounded-full shrink-0 self-center"
                    style={{
                      background: s.applicable
                        ? "var(--accent)"
                        : s.installed
                          ? "var(--text-3)"
                          : "transparent",
                      border: s.installed ? "none" : "1px solid var(--border-strong)",
                    }}
                  />
                  <span className="text-[12px]" style={{ color: "var(--text-2)" }}>
                    {s.label}
                  </span>
                  <span className="mono ml-auto text-[11px]" style={{ color: "var(--text-3)" }}>
                    {s.applicable ? "ready" : s.installed ? "not used here" : "not installed"}
                  </span>
                </div>
              ))
            )}
          </div>
          <p className="text-[11px] leading-relaxed mt-2" style={{ color: "var(--text-3)" }}>
            Servers are only ever looked for on your <span className="mono">PATH</span>,
            and only started for a language this repository actually contains.
            Note that some — rust-analyzer especially — build an index under{" "}
            <span className="mono">target/</span> while they work.
          </p>

          <div
            className="text-[10px] font-semibold tracking-[0.12em] uppercase mt-6 mb-2"
            style={{ color: "var(--text-3)" }}
          >
            Keyboard
          </div>
          {groups.map((group) => (
            <div key={group} className="mb-3">
              <div className="text-[11px] mb-1" style={{ color: "var(--text-3)" }}>
                {group}
              </div>
              {SHORTCUTS.filter((s) => s.group === group).map((s) => (
                <div
                  key={s.id}
                  className="flex items-baseline gap-3 py-0.5"
                  style={{ color: "var(--text-2)" }}
                >
                  <kbd
                    className="mono px-1.5 rounded text-[11px]"
                    style={{
                      background: "var(--input)",
                      border: "1px solid var(--border)",
                      color: "var(--text)",
                    }}
                  >
                    {shortcutLabel(s.keys)}
                  </kbd>
                  <span className="text-[12px]">{s.label}</span>
                </div>
              ))}
            </div>
          ))}
          <p className="text-[11px] mt-1" style={{ color: "var(--text-3)" }}>
            In the graph and the matrix: drag to pan, scroll to zoom.
          </p>

          <div
            className="text-[10px] font-semibold tracking-[0.12em] uppercase mt-6 mb-2"
            style={{ color: "var(--text-3)" }}
          >
            About
          </div>
          <p className="text-[12px] leading-relaxed" style={{ color: "var(--text-2)" }}>
            ZAtlas reads your code and your git history and never modifies either.
            The only thing it writes is its own cache in{" "}
            <span className="mono">.zatlas/</span> inside the repository, which
            ignores itself so it never appears in <span className="mono">git status</span>.
            Delete it any time; it is rebuilt on the next scan.
          </p>
          <p className="text-[12px] leading-relaxed mt-2" style={{ color: "var(--text-2)" }}>
            No telemetry, no account, no network. GPL-3.0-or-later.
          </p>

          <div className="-mx-2 mt-3">
            {LINKS.map((link) => (
              <LinkRow key={link.href} link={link} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
