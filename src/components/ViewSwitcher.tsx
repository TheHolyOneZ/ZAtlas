import { GitBranch, Grid3x3, LayoutGrid, LineChart } from "lucide-react";
import type { ComponentType } from "react";

import { CornerTicks } from "./ui";
import { useViewStore, VIEWS, type ViewId } from "../store/useViewStore";

const ICONS: Record<ViewId, ComponentType<{ size?: number }>> = {
  graph: GitBranch,
  treemap: LayoutGrid,
  matrix: Grid3x3,
  timeline: LineChart,
};


export default function ViewSwitcher() {
  const { view, setView } = useViewStore();

  return (
    <div
      className="flex items-stretch h-full"
      role="tablist"
      aria-label="View"
    >
      {VIEWS.map((v) => {
        const on = view === v.id;
        const Icon = ICONS[v.id];
        return (
          <button
            key={v.id}
            type="button"
            role="tab"
            aria-selected={on}
            onClick={() => setView(v.id)}
            title={`${v.label}  ·  ${v.key}`}
            className="relative flex items-center gap-2 px-3.5 h-full text-[12px] transition-colors duration-100 group"
            style={{ color: on ? "var(--accent)" : "var(--text-3)" }}
          >
            <Icon size={13} />
            <span className={on ? "" : "group-hover:text-[var(--text-2)]"}>{v.label}</span>


            <span
              className="mono text-[9px] opacity-0 group-hover:opacity-100 transition-opacity duration-150"
              style={{ color: "var(--text-3)" }}
            >
              {v.key}
            </span>

            {on && (
              <>
                <CornerTicks inset={4} />
                <span
                  aria-hidden
                  className="absolute left-2 right-2 bottom-0 h-[2px]"
                  style={{ background: "var(--accent)" }}
                />
              </>
            )}
          </button>
        );
      })}
    </div>
  );
}
