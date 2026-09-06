

import { worldToScreen, type Viewport } from "./viewport";

export interface SvgNode {
  x: number;
  y: number;
  r: number;
  fill: string;
  label?: string;
}

export interface SvgEdge {
  x1: number;
  y1: number;
  x2: number;
  y2: number;


  weak: boolean;
}

export interface SvgTheme {
  background: string;
  edge: string;

  weak: string;
  text: string;

  halo: string;
}

export interface SvgOptions {
  width: number;
  height: number;
  theme: SvgTheme;

  caption?: string;
}


export function escapeXml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}


function n(value: number): string {
  return Number.isFinite(value) ? String(Math.round(value * 100) / 100) : "0";
}

export function renderSvg(
  nodes: SvgNode[],
  edges: SvgEdge[],
  opts: SvgOptions,
): string {
  const { width, height, theme } = opts;
  const parts: string[] = [];

  parts.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${n(width)}" height="${n(
      height,
    )}" viewBox="0 0 ${n(width)} ${n(height)}" font-family="ui-monospace, monospace">`,
  );
  parts.push(`<rect width="100%" height="100%" fill="${escapeXml(theme.background)}"/>`);


  const plain: string[] = [];
  const weak: string[] = [];
  for (const e of edges) {
    const d = `M${n(e.x1)} ${n(e.y1)}L${n(e.x2)} ${n(e.y2)}`;
    (e.weak ? weak : plain).push(d);
  }
  if (plain.length > 0) {
    parts.push(
      `<path d="${plain.join("")}" stroke="${escapeXml(
        theme.edge,
      )}" stroke-width="0.75" stroke-opacity="0.45" fill="none"/>`,
    );
  }
  if (weak.length > 0) {
    parts.push(
      `<path d="${weak.join("")}" stroke="${escapeXml(
        theme.weak,
      )}" stroke-width="1" stroke-dasharray="4 3" fill="none"/>`,
    );
  }

  for (const node of nodes) {
    parts.push(
      `<circle cx="${n(node.x)}" cy="${n(node.y)}" r="${n(node.r)}" fill="${escapeXml(
        node.fill,
      )}"/>`,
    );
  }


  for (const node of nodes) {
    if (!node.label) continue;
    const x = n(node.x);
    const y = n(node.y - node.r - 4);
    const text = escapeXml(node.label);
    parts.push(
      `<text x="${x}" y="${y}" font-size="11" text-anchor="middle" stroke="${escapeXml(
        theme.halo,
      )}" stroke-width="2.5" paint-order="stroke" fill="${escapeXml(
        theme.text,
      )}">${text}</text>`,
    );
  }

  if (opts.caption) {
    parts.push(
      `<text x="10" y="${n(height - 10)}" font-size="10" fill="${escapeXml(
        theme.text,
      )}" opacity="0.6">${escapeXml(opts.caption)}</text>`,
    );
  }

  parts.push("</svg>");
  return parts.join("\n");
}


export function projectGraph(
  positions: Float32Array,
  radii: Float32Array,
  edges: { from: number; to: number; kind: string }[],
  viewport: Viewport,
  fill: (index: number) => string,
  visible: (index: number) => boolean,
  label: (index: number) => string | undefined,
): { nodes: SvgNode[]; edges: SvgEdge[] } {
  const count = Math.floor(positions.length / 2);
  const outNodes: SvgNode[] = [];
  const at: ([number, number] | null)[] = new Array(count).fill(null);

  for (let i = 0; i < count; i++) {
    if (!visible(i)) continue;
    const [sx, sy] = worldToScreen(viewport, positions[i * 2]!, positions[i * 2 + 1]!);
    at[i] = [sx, sy];
    outNodes.push({
      x: sx,
      y: sy,
      r: Math.max(1.5, radii[i]! * Math.pow(Math.max(viewport.zoom, 1e-6), 0.55)),
      fill: fill(i),
      label: label(i),
    });
  }

  const outEdges: SvgEdge[] = [];
  for (const e of edges) {


    if (e.kind === "coChange") continue;
    const a = at[e.from];
    const b = at[e.to];
    if (!a || !b) continue;
    outEdges.push({
      x1: a[0],
      y1: a[1],
      x2: b[0],
      y2: b[1],
      weak: e.kind === "viaBarrel",
    });
  }

  return { nodes: outNodes, edges: outEdges };
}


export function serialiseSvgElement(
  svg: SVGSVGElement,
  background: string,
  resolve: (name: string) => string,
): string {
  const clone = svg.cloneNode(true) as SVGSVGElement;
  const width = svg.getAttribute("width") ?? String(svg.clientWidth || 0);
  const height = svg.getAttribute("height") ?? String(svg.clientHeight || 0);

  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("width", width);
  clone.setAttribute("height", height);
  if (!clone.getAttribute("viewBox")) {
    clone.setAttribute("viewBox", `0 0 ${width} ${height}`);
  }

  for (const el of [clone, ...Array.from(clone.querySelectorAll("*"))]) {
    for (const attr of Array.from(el.attributes)) {
      if (attr.value.includes("var(--")) {
        el.setAttribute(attr.name, substituteVars(attr.value, resolve));
      }
    }


    const style = el.getAttribute("style");
    if (style?.includes("var(--")) {
      el.setAttribute("style", substituteVars(style, resolve));
    }
  }

  const ground =
    `<rect width="100%" height="100%" fill="${escapeXml(background)}"/>`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${escapeXml(width)}" height="${escapeXml(
    height,
  )}" viewBox="${escapeXml(clone.getAttribute("viewBox") ?? "")}">${ground}${clone.innerHTML}</svg>`;
}


export function substituteVars(
  value: string,
  resolve: (name: string) => string,
): string {
  return value.replace(/var\(\s*(--[\w-]+)\s*(?:,([^)]*))?\)/g, (_, name, fallback) => {
    const resolved = resolve(name).trim();
    if (resolved) return resolved;


    return (fallback ?? "").trim() || "none";
  });
}
