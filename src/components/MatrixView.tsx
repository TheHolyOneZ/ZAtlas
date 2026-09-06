import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Empty, Spinner } from "./ui";
import { useElementSize } from "../hooks/useElementSize";
import { severityCss } from "../lib/severity";
import { api } from "../lib/tauri";
import {
  IDENTITY,
  fitBox,
  pan,
  screenToWorld,
  worldToScreen,
  zoomAt,
  type Viewport,
} from "../lib/viewport";
import { useCaptureStore } from "../store/useCaptureStore";
import { useGraphStore } from "../store/useGraphStore";
import { useRepoStore } from "../store/useRepoStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { useViewStore } from "../store/useViewStore";


const CELL = 14;

const GUTTER = 190;

const LABEL_MIN_PX = 9;


const BAND_LABEL_PX = 150;


export default function MatrixView() {
  const { ref: wrapRef, size } = useElementSize<HTMLDivElement>();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);


  const setCapture = useCaptureStore((s) => s.setCapture);
  useEffect(() => {
    setCapture({ view: "matrix", canvas: () => canvasRef.current, toSvg: null });
    return () => setCapture(null);
  }, [setCapture]);
  const { graph } = useGraphStore();
  const version = useRepoStore((s) => s.version);
  const { selected, select } = useSelectionStore();
  const filters = useViewStore();

  const [order, setOrder] = useState<number[]>([]);
  const [loading, setLoading] = useState(false);
  const [viewport, setViewport] = useState<Viewport>(IDENTITY);
  const [hover, setHover] = useState<{ row: number; col: number } | null>(null);


  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api
      .dsmOrder()
      .then((o) => {
        if (!cancelled) setOrder(o);
      })
      .catch(() => {
        if (!cancelled) setOrder([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [version]);

  const rows = useMemo(() => {
    if (!graph) return [];
    const q = filters.query.toLowerCase();
    return order.filter((id) => {
      const n = graph.nodes[id];
      if (!n) return false;
      if (n.loc < filters.minLoc) return false;
      if (q && !n.path.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [order, graph, filters.query, filters.minLoc]);


  const bands = useMemo(() => {
    if (!graph) return [] as { start: number; end: number; label: string }[];
    const out: { start: number; end: number; label: string }[] = [];
    for (let r = 0; r < rows.length; r++) {
      const node = graph.nodes[rows[r]!];
      const label = node ? (graph.modules[node.module]?.path ?? "") : "";
      const last = out[out.length - 1];
      if (last && last.label === label) last.end = r;
      else out.push({ start: r, end: r, label });
    }
    return out;
  }, [graph, rows]);


  const marks = useMemo(() => {
    if (!graph) return new Set<number>();
    const index = new Map<number, number>();
    rows.forEach((id, i) => index.set(id, i));
    const out = new Set<number>();
    for (const e of graph.edges) {
      if (e.kind === "coChange") continue;
      const r = index.get(e.from);
      const c = index.get(e.to);
      if (r === undefined || c === undefined) continue;
      out.add(r * rows.length + c);
    }
    return out;
  }, [graph, rows]);


  useEffect(() => {
    if (rows.length === 0 || size.w === 0) return;
    const extent = rows.length * CELL;
    setViewport(
      fitBox(
        { minX: 0, minY: 0, maxX: GUTTER + extent, maxY: extent },
        size.w,
        size.h,
        24,
      ),
    );
  }, [rows.length, size.w, size.h]);


  const cellAtPoint = useCallback(
    (sx: number, sy: number): { row: number; col: number } | null => {
      const [wx, wy] = screenToWorld(viewport, sx, sy);
      const col = Math.floor((wx - GUTTER) / CELL);
      const row = Math.floor(wy / CELL);
      if (wx < GUTTER) return null;
      if (row < 0 || row >= rows.length || col < 0 || col >= rows.length) return null;
      return { row, col };
    },
    [viewport, rows.length],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || size.w === 0 || !graph) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.w, size.h);

    const css = getComputedStyle(document.documentElement);
    const isLight = document.documentElement.dataset.theme === "topographic-light";
    const grid = css.getPropertyValue("--border").trim() || "#1e2b29";
    const text = css.getPropertyValue("--text-3").trim() || "#5d6f6b";
    const accent = css.getPropertyValue("--accent").trim() || "#2dd4bf";
    const cycle = css.getPropertyValue("--cycle").trim() || "#f87171";

    const px = CELL * viewport.zoom;
    const n = rows.length;


    const [wx0, wy0] = screenToWorld(viewport, 0, 0);
    const [wx1, wy1] = screenToWorld(viewport, size.w, size.h);
    const firstRow = Math.max(0, Math.floor(wy0 / CELL));
    const lastRow = Math.min(n - 1, Math.ceil(wy1 / CELL));
    const firstCol = Math.max(0, Math.floor((wx0 - GUTTER) / CELL));
    const lastCol = Math.min(n - 1, Math.ceil((wx1 - GUTTER) / CELL));


    const [mLeft, mTop] = worldToScreen(viewport, GUTTER, 0);
    const [mRight, mBottom] = worldToScreen(viewport, GUTTER + n * CELL, n * CELL);
    const mWidth = mRight - mLeft;
    const mHeight = mBottom - mTop;


    ctx.strokeStyle = grid;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(mLeft, mTop);
    ctx.lineTo(mRight, mBottom);
    ctx.stroke();


    const selectedIndex = selected === null ? -1 : rows.indexOf(selected);
    if (selectedIndex >= 0) {
      const [, sy] = worldToScreen(viewport, 0, selectedIndex * CELL);
      const [sx] = worldToScreen(viewport, GUTTER + selectedIndex * CELL, 0);
      ctx.fillStyle = accent;
      ctx.globalAlpha = 0.08;
      ctx.fillRect(mLeft, sy, mWidth, px);
      ctx.fillRect(sx, mTop, px, mHeight);
      ctx.globalAlpha = 1;
    }

    for (let r = firstRow; r <= lastRow; r++) {
      const [, sy] = worldToScreen(viewport, 0, r * CELL);
      for (let c = firstCol; c <= lastCol; c++) {
        if (!marks.has(r * n + c)) continue;
        const [sx] = worldToScreen(viewport, GUTTER + c * CELL, 0);

        const back = c > r;
        ctx.fillStyle = back
          ? cycle
          : filters.showHotspots
            ? severityCss(graph.nodes[rows[r]!]?.heat ?? 0, isLight)
            : accent;
        ctx.globalAlpha = back ? 0.95 : 0.8;
        const inset = px > 4 ? 0.5 : 0;
        ctx.fillRect(sx + inset, sy + inset, px - inset * 2, px - inset * 2);
      }
    }
    ctx.globalAlpha = 1;


    if (px < LABEL_MIN_PX) {
      const [gx] = worldToScreen(viewport, GUTTER, 0);
      const gutterPx = GUTTER * viewport.zoom;


      const budget = Math.min(gx, Math.max(gutterPx, BAND_LABEL_PX));
      const left = Math.max(0, gx - budget);
      const visible = Math.max(0, gx - left);
      ctx.save();
      ctx.beginPath();
      ctx.rect(left, 0, visible, size.h);
      ctx.clip();
      ctx.font = "10px 'JetBrains Mono Variable', ui-monospace, monospace";
      ctx.textAlign = "right";

      for (const band of bands) {
        const [, top] = worldToScreen(viewport, 0, band.start * CELL);
        const [, bottom] = worldToScreen(viewport, 0, (band.end + 1) * CELL);
        if (bottom < 0 || top > size.h) continue;
        const height = bottom - top;


        if (height < 14 || visible < 30) continue;


        ctx.globalAlpha = 0.45;
        ctx.fillStyle = accent;
        ctx.fillRect(gx - 3, top + 1, 2, Math.max(1, height - 2));
        ctx.globalAlpha = 1;

        ctx.fillStyle = text;
        const name = band.label || "(root)";
        const max = Math.max(3, Math.floor((visible - 12) / 6));
        ctx.fillText(
          name.length > max ? `…${name.slice(-max + 1)}` : name,
          gx - 9,
          top + height / 2 + 3.5,
        );
      }
      ctx.restore();
    }


    if (px >= LABEL_MIN_PX) {
      const [gx] = worldToScreen(viewport, GUTTER, 0);
      const gutterPx = GUTTER * viewport.zoom;


      const left = Math.max(0, gx - gutterPx);
      const visible = Math.max(0, gx - left);
      ctx.save();
      ctx.beginPath();
      ctx.rect(left, 0, gutterPx, size.h);
      ctx.clip();
      ctx.font = `${Math.min(12, Math.max(9, px * 0.72))}px 'JetBrains Mono Variable', ui-monospace, monospace`;
      ctx.textAlign = "right";
      for (let r = firstRow; r <= lastRow; r++) {
        const node = graph.nodes[rows[r]!];
        if (!node) continue;
        const [, sy] = worldToScreen(viewport, 0, r * CELL);
        ctx.fillStyle = rows[r] === selected ? accent : text;
        const max = Math.max(3, Math.floor((visible - 8) / Math.max(4, px * 0.42)));
        const label =
          node.path.length > max ? `…${node.path.slice(-max + 1)}` : node.path;
        ctx.fillText(label, gx - 6, sy + px * 0.75);
      }
      ctx.restore();
    }


    if (hover) {
      const [hx, hy] = worldToScreen(
        viewport,
        GUTTER + hover.col * CELL,
        hover.row * CELL,
      );
      ctx.strokeStyle = accent;
      ctx.lineWidth = 1;
      ctx.strokeRect(hx - 0.5, hy - 0.5, px + 1, px + 1);


      ctx.globalAlpha = 0.25;
      ctx.beginPath();
      ctx.moveTo(mLeft, hy + px / 2);
      ctx.lineTo(hx, hy + px / 2);
      ctx.moveTo(hx + px / 2, mTop);
      ctx.lineTo(hx + px / 2, hy);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }, [graph, rows, marks, bands, size, viewport, selected, hover, filters.showHotspots]);

  const dragRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const empty = rows.length === 0;

  return (
    <div ref={wrapRef} className="relative h-full w-full overflow-hidden">
      {loading && (
        <div className="absolute inset-0 grid place-items-center z-10">
          <Spinner label="ordering" />
        </div>
      )}
      {!loading && empty && (
        <div className="absolute inset-0 z-10">
          <Empty
            title="Nothing to plot"
            hint="Every file was filtered out, or the repository has no dependencies to show."
          />
        </div>
      )}
      <canvas
        ref={canvasRef}
        style={{
          width: size.w,
          height: size.h,
          cursor: dragRef.current ? "grabbing" : hover ? "crosshair" : "grab",
        }}
        onMouseDown={(e) => {
          dragRef.current = { x: e.clientX, y: e.clientY, moved: false };
        }}
        onMouseMove={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          if (dragRef.current) {
            const dx = e.clientX - dragRef.current.x;
            const dy = e.clientY - dragRef.current.y;
            if (Math.abs(dx) + Math.abs(dy) > 2) dragRef.current.moved = true;
            dragRef.current.x = e.clientX;
            dragRef.current.y = e.clientY;
            setViewport((v) => pan(v, dx, dy));
            return;
          }
          setHover(cellAtPoint(e.clientX - rect.left, e.clientY - rect.top));
        }}
        onMouseUp={(e) => {
          const wasDrag = dragRef.current?.moved ?? false;
          dragRef.current = null;
          if (wasDrag) return;
          const rect = e.currentTarget.getBoundingClientRect();
          const cell = cellAtPoint(e.clientX - rect.left, e.clientY - rect.top);
          const id = cell ? rows[cell.row] : undefined;
          if (id !== undefined) void select(id);
        }}
        onMouseLeave={() => {
          dragRef.current = null;
          setHover(null);
        }}
        onWheel={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          const factor = Math.exp(-e.deltaY * 0.0015);
          setViewport((v) =>
            zoomAt(v, e.clientX - rect.left, e.clientY - rect.top, factor),
          );
        }}
      />

      {hover && graph && (
        <div
          className="absolute bottom-2 left-2 px-2 py-1 rounded-[3px] pointer-events-none max-w-[72%]"
          style={{
            background: "var(--raised)",
            border: "1px solid var(--border)",
            boxShadow: "var(--shadow)",
          }}
        >
          <div className="mono truncate text-[11px]" style={{ color: "var(--text)" }}>
            {graph.nodes[rows[hover.row]!]?.path}
          </div>
          <div className="mono truncate text-[11px]" style={{ color: "var(--text-3)" }}>
            {marks.has(hover.row * rows.length + hover.col)
              ? "imports"
              : "does not import"}{" "}
            {graph.nodes[rows[hover.col]!]?.path}
            {hover.col > hover.row && marks.has(hover.row * rows.length + hover.col) && (
              <span style={{ color: "var(--cycle)" }}> — a back-edge</span>
            )}
          </div>
        </div>
      )}

      {!empty && (
        <div
          className="mono absolute top-2 right-2 px-2 py-0.5 rounded-[3px] text-[10px] pointer-events-none"
          style={{ background: "var(--raised)", color: "var(--text-3)" }}
        >
          {rows.length}×{rows.length} · drag to pan · scroll to zoom
        </div>
      )}
    </div>
  );
}
