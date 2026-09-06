import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Empty, Spinner } from "./ui";
import { buildContours, type ContourBand } from "../lib/contours";
import { projectGraph, renderSvg } from "../lib/graphSvg";
import { severityCss } from "../lib/severity";
import {
  fitBox,
  nodeScreenRadius,
  pan,
  trimmedBox,
  visibleWorldRect,
  worldToScreen,
  zoomAt,
  distanceToSegment,
} from "../lib/viewport";
import { useElementSize } from "../hooks/useElementSize";
import { useCaptureStore } from "../store/useCaptureStore";
import { useGraphStore } from "../store/useGraphStore";
import { useSettingsStore } from "../store/useSettingsStore";
import { useViewportStore } from "../store/useViewportStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { useViewStore } from "../store/useViewStore";


const LABEL_LIMIT = 90;

const LABEL_MIN_RADIUS = 6;
const EDGE_LIMIT = 12_000;


const SETTLE_MS = 800;

export default function GraphView() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const { ref: wrapRef, size } = useElementSize<HTMLDivElement>();
  const { graph, positions, radii, layout, loading } = useGraphStore();
  const { selected, highlighted, select, selectEdge } = useSelectionStore();
  const filters = useViewStore();
  const contoursOn = useSettingsStore((s) => s.contours);


  const viewport = useViewportStore((s) => s.viewport);
  const setViewport = useViewportStore((s) => s.setViewport);
  const updateViewport = useViewportStore((s) => s.updateViewport);
  const restored = useViewportStore((s) => s.restored);
  const [hover, setHover] = useState<number | null>(null);


  const contours: ContourBand[] = useMemo(() => {
    if (!contoursOn || !positions || !layout) return [];
    const n = positions.length / 2;
    const xs = new Float32Array(n);
    const ys = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      xs[i] = positions[i * 2]!;
      ys[i] = positions[i * 2 + 1]!;
    }


    const padX = (layout.maxX - layout.minX) * 0.12 + 1;
    const padY = (layout.maxY - layout.minY) * 0.12 + 1;
    return buildContours(xs, ys, {
      minX: layout.minX - padX,
      minY: layout.minY - padY,
      maxX: layout.maxX + padX,
      maxY: layout.maxY + padY,
    });
  }, [contoursOn, positions, layout]);


  const settleRef = useRef<{ start: number; raf: number } | null>(null);
  const [settle, setSettle] = useState(1);


  const lastRestore = useRef(restored);
  useEffect(() => {
    if (!layout || !positions || size.w === 0) return;
    if (lastRestore.current !== restored) {
      lastRestore.current = restored;
      return;
    }
    setViewport(fitBox(trimmedBox(positions), size.w, size.h));
    if (settleRef.current) cancelAnimationFrame(settleRef.current.raf);
    const start = performance.now();
    const step = () => {
      const t = Math.min(1, (performance.now() - start) / SETTLE_MS);

      setSettle(1 - Math.pow(1 - t, 3));
      if (t < 1) {
        settleRef.current = { start, raf: requestAnimationFrame(step) };
      } else {
        settleRef.current = null;
      }
    };
    settleRef.current = { start, raf: requestAnimationFrame(step) };
    return () => {
      if (settleRef.current) cancelAnimationFrame(settleRef.current.raf);
      settleRef.current = null;
    };
  }, [layout, positions, size.w, size.h, restored, setViewport]);

  const visible = useCallback(
    (i: number): boolean => {
      if (!graph) return false;
      const n = graph.nodes[i];
      if (!n) return false;
      if (filters.minLoc > 0 && n.loc < filters.minLoc) return false;
      if (filters.query && !n.path.toLowerCase().includes(filters.query.toLowerCase()))
        return false;
      if (filters.showOrphans && n.fanIn > 0) return false;
      return true;
    },
    [graph, filters.minLoc, filters.query, filters.showOrphans],
  );


  const setCapture = useCaptureStore((s) => s.setCapture);
  useEffect(() => {
    if (!graph || !positions || !radii) return;
    setCapture({
      view: "graph",
      canvas: () => canvasRef.current,
      toSvg: () => {
        const css = getComputedStyle(document.documentElement);
        const token = (name: string, fallback: string) =>
          css.getPropertyValue(name).trim() || fallback;
        const isLight =
          document.documentElement.dataset.theme === "topographic-light";
        const accent = token("--accent", "#2dd4bf");
        const { nodes, edges } = projectGraph(
          positions,
          radii,
          graph.edges,
          viewport,
          (i) =>
            filters.showHotspots
              ? severityCss(graph.nodes[i]?.heat ?? 0, isLight)
              : accent,
          visible,


          (i) => {
            const n = graph.nodes[i];
            if (!n) return undefined;
            return n.fanIn >= 4 ? n.name : undefined;
          },
        );
        return renderSvg(nodes, edges, {
          width: size.w,
          height: size.h,
          theme: {
            background: token("--canvas", "#0b1211"),
            edge: token("--edge", "#43a99b"),
            weak: token("--cycle", "#f87171"),
            text: token("--text-2", "#93a5a1"),
            halo: token("--canvas", "#0b1211"),
          },
        });
      },
    });
    return () => setCapture(null);
  }, [graph, positions, radii, viewport, size.w, size.h, visible, filters.showHotspots, setCapture]);


  const hitTest = useCallback(
    (sx: number, sy: number): number | null => {
      if (!graph || !positions || !radii) return null;
      let best: number | null = null;
      let bestDist = Infinity;
      for (let i = 0; i < graph.nodes.length; i++) {
        if (!visible(i)) continue;
        const [nx, ny] = worldToScreen(
          viewport,
          positions[i * 2]!,
          positions[i * 2 + 1]!,
        );


        const r = nodeScreenRadius(radii[i]!, viewport.zoom) + 2;
        const d = (nx - sx) ** 2 + (ny - sy) ** 2;
        if (d <= r * r && d < bestDist) {
          bestDist = d;
          best = i;
        }
      }
      return best;
    },
    [graph, positions, radii, viewport, visible],
  );


  const hitTestEdge = useCallback(
    (sx: number, sy: number): { from: number; to: number } | null => {
      if (!graph || !positions) return null;
      const THRESHOLD = 5;
      let best: { from: number; to: number } | null = null;
      let bestDist = THRESHOLD;
      for (const e of graph.edges) {
        if (e.kind === "coChange") continue;
        if (!visible(e.from) || !visible(e.to)) continue;
        const [ax, ay] = worldToScreen(viewport, positions[e.from * 2]!, positions[e.from * 2 + 1]!);
        const [bx, by] = worldToScreen(viewport, positions[e.to * 2]!, positions[e.to * 2 + 1]!);
        const d = distanceToSegment(sx, sy, ax, ay, bx, by);
        if (d < bestDist) {
          bestDist = d;
          best = { from: e.from, to: e.to };
        }
      }
      return best;
    },
    [graph, positions, viewport, visible],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !graph || !positions || !radii || size.w === 0) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const css = getComputedStyle(document.documentElement);
    const isLight = document.documentElement.dataset.theme === "topographic-light";
    const bg = css.getPropertyValue("--canvas").trim() || "#0b1211";
    const edgeColor = css.getPropertyValue("--edge").trim() || "#43a99b";
    const cycleColor = css.getPropertyValue("--cycle").trim() || "#f87171";
    const textColor = css.getPropertyValue("--text-2").trim() || "#93a5a1";
    const accent = css.getPropertyValue("--accent").trim() || "#2dd4bf";


    ctx.clearRect(0, 0, size.w, size.h);

    const rect = visibleWorldRect(viewport, size.w, size.h);
    const margin = 80;
    const onScreen = (i: number) => {
      const px = positions[i * 2]!;
      const py = positions[i * 2 + 1]!;
      return (
        px >= rect.minX - margin &&
        px <= rect.maxX + margin &&
        py >= rect.minY - margin &&
        py <= rect.maxY + margin
      );
    };


    const cx = (layout!.minX + layout!.maxX) / 2;
    const cy = (layout!.minY + layout!.maxY) / 2;
    const at = (i: number): [number, number] => {
      const px = positions[i * 2]!;
      const py = positions[i * 2 + 1]!;
      if (settle >= 1) return [px, py];
      return [cx + (px - cx) * settle, cy + (py - cy) * settle];
    };


    if (contours.length > 0) {
      ctx.strokeStyle = css.getPropertyValue("--contour").trim() || "#43a99b";
      ctx.lineWidth = 1;
      ctx.lineCap = "round";
      for (const band of contours) {


        ctx.globalAlpha = 0.05 + band.level * 0.1;
        ctx.beginPath();
        const seg = band.segments;
        for (let i = 0; i < seg.length; i += 4) {
          const ax = seg[i]!;
          const ay = seg[i + 1]!;
          const bx = seg[i + 2]!;
          const by = seg[i + 3]!;


          if (
            (ax < rect.minX && bx < rect.minX) ||
            (ax > rect.maxX && bx > rect.maxX) ||
            (ay < rect.minY && by < rect.minY) ||
            (ay > rect.maxY && by > rect.maxY)
          ) {
            continue;
          }
          const [sx1, sy1] = worldToScreen(viewport, ax, ay);
          const [sx2, sy2] = worldToScreen(viewport, bx, by);
          ctx.moveTo(sx1, sy1);
          ctx.lineTo(sx2, sy2);
        }
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
    }


    if (graph.edges.length <= EDGE_LIMIT) {


      const drawn = graph.edges.filter((e) => e.kind !== "coChange").length;
      const edgeAlpha = Math.max(
        0.32,
        Math.min(0.85, 1.45 / Math.log10(drawn + 10)),
      );

      ctx.lineWidth = 1;
      ctx.globalAlpha = edgeAlpha;
      ctx.strokeStyle = edgeColor;
      ctx.beginPath();
      for (const e of graph.edges) {
        if (e.kind === "coChange") continue;
        if (!visible(e.from) || !visible(e.to)) continue;
        if (!onScreen(e.from) && !onScreen(e.to)) continue;


        if (highlighted.size > 0 && !(highlighted.has(e.from) && highlighted.has(e.to))) {
          continue;
        }
        const [ax, ay] = worldToScreen(viewport, ...at(e.from));
        const [bx, by] = worldToScreen(viewport, ...at(e.to));
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
      }
      ctx.stroke();


      if (highlighted.size > 0) {


        ctx.globalAlpha = edgeAlpha * 0.34;
        ctx.beginPath();
        for (const e of graph.edges) {
          if (e.kind === "coChange") continue;
          if (!visible(e.from) || !visible(e.to)) continue;
          if (!onScreen(e.from) && !onScreen(e.to)) continue;
          if (highlighted.has(e.from) && highlighted.has(e.to)) continue;
          const [ax, ay] = worldToScreen(viewport, ...at(e.from));
          const [bx, by] = worldToScreen(viewport, ...at(e.to));
          ctx.moveTo(ax, ay);
          ctx.lineTo(bx, by);
        }
        ctx.stroke();
      }

      if (filters.showCycles) {
        ctx.globalAlpha = 0.9;
        ctx.strokeStyle = cycleColor;
        ctx.setLineDash([4, 3]);
        ctx.beginPath();
        for (const e of graph.edges) {
          if (e.kind !== "viaBarrel") continue;
          if (!visible(e.from) || !visible(e.to)) continue;
          const [ax, ay] = worldToScreen(viewport, ...at(e.from));
          const [bx, by] = worldToScreen(viewport, ...at(e.to));
          ctx.moveTo(ax, ay);
          ctx.lineTo(bx, by);
        }
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }


    ctx.globalAlpha = 1;
    const dimmed = highlighted.size > 0;
    for (let i = 0; i < graph.nodes.length; i++) {
      if (!visible(i) || !onScreen(i)) continue;
      const n = graph.nodes[i]!;
      const [wx, wy] = at(i);
      const [sx, sy] = worldToScreen(viewport, wx, wy);
      const r = nodeScreenRadius(radii[i]!, viewport.zoom);

      const isHot = dimmed && (highlighted.has(i) || i === selected);
      ctx.globalAlpha = dimmed && !isHot ? 0.32 : 1;
      ctx.fillStyle = filters.showHotspots ? severityCss(n.heat, isLight) : accent;
      ctx.beginPath();
      ctx.arc(sx, sy, r, 0, Math.PI * 2);
      ctx.fill();

      if (i === selected || i === hover) {
        ctx.globalAlpha = 1;
        ctx.strokeStyle = accent;
        ctx.lineWidth = 2;
        ctx.stroke();


        if (i === hover) {
          ctx.beginPath();
          ctx.arc(sx, sy, r + 4, 0, Math.PI * 2);
          ctx.globalAlpha = 0.4;
          ctx.lineWidth = 1;
          ctx.stroke();
          ctx.globalAlpha = 1;
        }
      }


      if (i === selected) {
        const gap = r + 6;
        const arm = 5;
        ctx.globalAlpha = 1;
        ctx.strokeStyle = accent;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        for (const [dx, dy] of [
          [0, -1],
          [0, 1],
          [-1, 0],
          [1, 0],
        ] as const) {
          ctx.moveTo(sx + dx * gap, sy + dy * gap);
          ctx.lineTo(sx + dx * (gap + arm), sy + dy * (gap + arm));
        }
        ctx.stroke();
      }
    }


    ctx.globalAlpha = 1;
    ctx.font = "11px 'JetBrains Mono Variable', ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.lineWidth = 2.5;
    ctx.strokeStyle = bg;

    const candidates: number[] = [];
    for (let i = 0; i < graph.nodes.length; i++) {
      if (!visible(i) || !onScreen(i)) continue;
      if (dimmed && !highlighted.has(i)) continue;
      if (nodeScreenRadius(radii[i]!, viewport.zoom) < LABEL_MIN_RADIUS) continue;
      candidates.push(i);
    }


    candidates.sort((a, b) => graph.nodes[b]!.fanIn - graph.nodes[a]!.fanIn);

    const placed: { x0: number; y0: number; x1: number; y1: number }[] = [];
    let drawn = 0;
    for (const i of candidates) {
      if (drawn >= LABEL_LIMIT) break;
      const n = graph.nodes[i]!;
      const [sx, sy] = worldToScreen(viewport, ...at(i));
      const r = nodeScreenRadius(radii[i]!, viewport.zoom);
      const w = ctx.measureText(n.name).width;
      const box = {
        x0: sx - w / 2 - 2,
        y0: sy - r - 14,
        x1: sx + w / 2 + 2,
        y1: sy - r + 1,
      };
      const clashes = placed.some(
        (p) => box.x0 < p.x1 && box.x1 > p.x0 && box.y0 < p.y1 && box.y1 > p.y0,
      );
      if (clashes) continue;
      placed.push(box);
      drawn++;
      ctx.strokeText(n.name, sx, sy - r - 4);
      ctx.fillStyle = i === selected || i === hover ? accent : textColor;
      ctx.fillText(n.name, sx, sy - r - 4);
    }


    if (hover !== null && visible(hover) && onScreen(hover)) {
      const n = graph.nodes[hover]!;
      const [sx, sy] = worldToScreen(viewport, ...at(hover));
      const r = nodeScreenRadius(radii[hover]!, viewport.zoom);
      ctx.strokeText(n.name, sx, sy - r - 4);
      ctx.fillStyle = accent;
      ctx.fillText(n.name, sx, sy - r - 4);
    }
    ctx.globalAlpha = 1;
  }, [
    graph,
    positions,
    radii,
    layout,
    viewport,
    size,
    selected,
    hover,
    highlighted,
    settle,
    filters.showCycles,
    filters.showHotspots,
    contours,
    visible,
  ]);

  const dragRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  const empty = !graph || graph.nodes.length === 0;


  return (
    <div ref={wrapRef} className="relative h-full w-full overflow-hidden">
      {loading && (
        <div className="absolute inset-0 grid place-items-center z-10">
          <Spinner label="building the map" />
        </div>
      )}
      {!loading && empty && (
        <div className="absolute inset-0 z-10">
          <Empty title="No files to draw" hint="The scan found nothing to analyse." />
        </div>
      )}
      <canvas
        ref={canvasRef}
        style={{ width: size.w, height: size.h, cursor: hover !== null ? "pointer" : "grab" }}
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
            updateViewport((v) => pan(v, dx, dy));
            return;
          }
          setHover(hitTest(e.clientX - rect.left, e.clientY - rect.top));
        }}
        onMouseUp={(e) => {
          const wasDrag = dragRef.current?.moved ?? false;
          dragRef.current = null;
          if (wasDrag) return;
          const rect = e.currentTarget.getBoundingClientRect();
          const x = e.clientX - rect.left;
          const y = e.clientY - rect.top;
          const hit = hitTest(x, y);
          if (hit !== null) {
            void select(hit);
            return;
          }


          const edge = hitTestEdge(x, y);
          if (edge) {
            void selectEdge(edge.from, edge.to);
          } else {
            void select(null);
          }
        }}
        onMouseLeave={() => {
          dragRef.current = null;
          setHover(null);
        }}
        onWheel={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          const factor = Math.exp(-e.deltaY * 0.0015);
          updateViewport((v) => zoomAt(v, e.clientX - rect.left, e.clientY - rect.top, factor));
        }}
      />


      {graph && graph.nodes.length > 0 && (
        <div
          className="mono absolute top-2 right-2 px-2 py-0.5 rounded-[3px] text-[10px] pointer-events-none"
          style={{ background: "var(--raised)", color: "var(--text-3)" }}
        >
          {graph.nodes.length} nodes · drag to pan · scroll to zoom
        </div>
      )}

      {hover !== null && graph?.nodes[hover] && (
        <div
          className="absolute bottom-2 left-2 px-2 py-1 rounded pointer-events-none"
          style={{
            background: "var(--raised)",
            border: "1px solid var(--border)",
            boxShadow: "var(--shadow)",
          }}
        >
          <div className="mono" style={{ color: "var(--text)" }}>
            {graph.nodes[hover]!.path}
          </div>
          <div className="text-[11px]" style={{ color: "var(--text-3)" }}>
            {graph.nodes[hover]!.loc} LOC · {graph.nodes[hover]!.fanIn} dependents ·{" "}
            {graph.nodes[hover]!.fanOut} imports
          </div>
        </div>
      )}
    </div>
  );
}
