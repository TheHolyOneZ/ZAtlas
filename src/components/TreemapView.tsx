import { useEffect, useMemo, useRef, useState } from "react";

import { Empty } from "./ui";
import { severityCss } from "../lib/severity";
import {
  HEADER_HEIGHT,
  cellAt,
  layoutTreemap,
  type TreeInput,
} from "../lib/treemapLayout";
import { useElementSize } from "../hooks/useElementSize";
import { useCaptureStore } from "../store/useCaptureStore";
import { useGraphStore } from "../store/useGraphStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { useViewStore } from "../store/useViewStore";

export default function TreemapView() {
  const { ref: wrapRef, size } = useElementSize<HTMLDivElement>();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);


  const setCapture = useCaptureStore((s) => s.setCapture);
  useEffect(() => {
    setCapture({ view: "treemap", canvas: () => canvasRef.current, toSvg: null });
    return () => setCapture(null);
  }, [setCapture]);
  const { graph } = useGraphStore();
  const { selected, select } = useSelectionStore();
  const filters = useViewStore();
  const [hover, setHover] = useState<string | null>(null);

  const cells = useMemo(() => {
    if (!graph || size.w === 0) return [];
    const q = filters.query.toLowerCase();
    const input: TreeInput[] = graph.nodes
      .filter((n) => n.loc >= filters.minLoc)
      .filter((n) => !q || n.path.toLowerCase().includes(q))
      .map((n) => ({
        id: n.id,
        path: n.path,
        name: n.name,

        value: Math.max(n.loc, 1),
        heat: n.heat,
      }));
    return layoutTreemap(input, size.w, size.h);
  }, [graph, size, filters.minLoc, filters.query]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || size.w === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const css = getComputedStyle(document.documentElement);
    const isLight = document.documentElement.dataset.theme === "topographic-light";
    const bg = css.getPropertyValue("--canvas").trim() || "#0b1211";
    const border = css.getPropertyValue("--border").trim() || "#1e2b29";
    const text = css.getPropertyValue("--text-2").trim() || "#93a5a1";
    const accent = css.getPropertyValue("--accent").trim() || "#2dd4bf";

    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, size.w, size.h);


    const clipped = (
      x: number,
      y: number,
      w: number,
      h: number,
      draw: () => void,
    ) => {
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w, h);
      ctx.clip();
      draw();
      ctx.restore();
    };


    const fitText = (context: CanvasRenderingContext2D, text: string, max: number) => {
      if (context.measureText(text).width <= max) return text;
      let lo = 0;
      let hi = text.length;
      while (lo < hi) {
        const mid = Math.ceil((lo + hi) / 2);
        if (context.measureText(`${text.slice(0, mid)}…`).width <= max) lo = mid;
        else hi = mid - 1;
      }
      return lo > 0 ? `${text.slice(0, lo)}…` : "";
    };

    for (const c of cells.filter((x) => !x.isLeaf)) {
      const w = c.x1 - c.x0;
      const h = c.y1 - c.y0;
      ctx.fillStyle = border;
      ctx.globalAlpha = 0.25;
      ctx.fillRect(c.x0, c.y0, w, h);
      ctx.globalAlpha = 1;

      if (w > 40 && h >= HEADER_HEIGHT) {
        clipped(c.x0, c.y0, w, HEADER_HEIGHT, () => {
          ctx.font = "10px 'Inter Variable', system-ui, sans-serif";
          ctx.fillStyle = text;
          ctx.textAlign = "left";
          ctx.fillText(fitText(ctx, c.name, w - 8), c.x0 + 4, c.y0 + 10);
        });
      }
    }

    for (const c of cells.filter((x) => x.isLeaf)) {
      const w = c.x1 - c.x0;
      const h = c.y1 - c.y0;
      ctx.fillStyle = filters.showHotspots ? severityCss(c.heat, isLight) : accent;
      ctx.globalAlpha = c.heat > 0 ? 0.9 : 0.55;
      ctx.fillRect(c.x0, c.y0, w, h);
      ctx.globalAlpha = 1;

      if (c.id === selected) {
        ctx.strokeStyle = accent;
        ctx.lineWidth = 2;
        ctx.strokeRect(c.x0 + 1, c.y0 + 1, w - 2, h - 2);
      }


      if (w > 44 && h > 15) {
        clipped(c.x0, c.y0, w, h, () => {
          ctx.font = "10px 'JetBrains Mono Variable', ui-monospace, monospace";
          ctx.fillStyle = bg;
          ctx.textAlign = "left";
          ctx.fillText(fitText(ctx, c.name, w - 8), c.x0 + 4, c.y0 + 11);
        });
      }
    }
  }, [cells, size, selected, filters.showHotspots]);


  return (
    <div ref={wrapRef} className="relative h-full w-full overflow-hidden">
      {cells.length === 0 && (
        <div className="absolute inset-0 z-10">
          <Empty
            title="Nothing to lay out"
            hint="Every file was filtered out, or the repository has no source files."
          />
        </div>
      )}
      <canvas
        ref={canvasRef}
        style={{ width: size.w, height: size.h, cursor: "pointer" }}
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const c = cellAt(cells, e.clientX - r.left, e.clientY - r.top);
          setHover(c?.path ?? null);
        }}
        onMouseLeave={() => setHover(null)}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const c = cellAt(cells, e.clientX - r.left, e.clientY - r.top);
          if (c?.isLeaf) void select(c.id);
        }}
      />
      {hover && (
        <div
          className="mono absolute bottom-2 left-2 px-2 py-1 rounded pointer-events-none"
          style={{
            background: "var(--raised)",
            border: "1px solid var(--border)",
            color: "var(--text)",
            boxShadow: "var(--shadow)",
          }}
        >
          {hover}
        </div>
      )}
    </div>
  );
}
