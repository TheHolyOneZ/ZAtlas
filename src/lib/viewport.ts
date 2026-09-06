

export interface Viewport {
  x: number;
  y: number;
  zoom: number;
}

export const MIN_ZOOM = 0.02;
export const MAX_ZOOM = 24;

export const IDENTITY: Viewport = { x: 0, y: 0, zoom: 1 };


export const NODE_MIN_PX = 4.5;

export const NODE_MAX_PX = 26;


const NODE_ZOOM_RESPONSE = 0.55;


export function nodeScreenRadius(worldRadius: number, zoom: number): number {
  const scaled = worldRadius * Math.pow(Math.max(zoom, 1e-6), NODE_ZOOM_RESPONSE);
  return Math.min(NODE_MAX_PX, Math.max(NODE_MIN_PX, scaled));
}

export function worldToScreen(v: Viewport, wx: number, wy: number): [number, number] {
  return [wx * v.zoom + v.x, wy * v.zoom + v.y];
}

export function screenToWorld(v: Viewport, sx: number, sy: number): [number, number] {
  return [(sx - v.x) / v.zoom, (sy - v.y) / v.zoom];
}


export function zoomAt(v: Viewport, sx: number, sy: number, factor: number): Viewport {
  const zoom = clampZoom(v.zoom * factor);
  if (zoom === v.zoom) return v;
  const [wx, wy] = screenToWorld(v, sx, sy);
  return { zoom, x: sx - wx * zoom, y: sy - wy * zoom };
}

export function clampZoom(z: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
}

export function pan(v: Viewport, dx: number, dy: number): Viewport {
  return { ...v, x: v.x + dx, y: v.y + dy };
}


export function visibleWorldRect(
  v: Viewport,
  width: number,
  height: number,
): { minX: number; minY: number; maxX: number; maxY: number } {
  const [minX, minY] = screenToWorld(v, 0, 0);
  const [maxX, maxY] = screenToWorld(v, width, height);
  return { minX, minY, maxX, maxY };
}


export function trimmedBox(
  xy: Float32Array | number[],
  trim = 0.02,
): { minX: number; minY: number; maxX: number; maxY: number } {
  const n = Math.floor(xy.length / 2);
  if (n === 0) return { minX: 0, minY: 0, maxX: 1, maxY: 1 };

  const xs = new Float64Array(n);
  const ys = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    xs[i] = xy[i * 2] as number;
    ys[i] = xy[i * 2 + 1] as number;
  }
  xs.sort();
  ys.sort();


  const cut = n < 12 ? 0 : Math.min(Math.floor(n * trim), Math.floor((n - 1) / 2));
  const lo = cut;
  const hi = n - 1 - cut;

  return {
    minX: xs[lo] as number,
    minY: ys[lo] as number,
    maxX: xs[hi] as number,
    maxY: ys[hi] as number,
  };
}


export function fitBox(
  box: { minX: number; minY: number; maxX: number; maxY: number },
  width: number,
  height: number,
  padding = 48,
): Viewport {
  const bw = Math.max(box.maxX - box.minX, 1);
  const bh = Math.max(box.maxY - box.minY, 1);
  const zoom = clampZoom(
    Math.min((width - padding * 2) / bw, (height - padding * 2) / bh),
  );
  const cx = (box.minX + box.maxX) / 2;
  const cy = (box.minY + box.maxY) / 2;
  return { zoom, x: width / 2 - cx * zoom, y: height / 2 - cy * zoom };
}


export function distanceToSegment(
  px: number,
  py: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
): number {
  const dx = bx - ax;
  const dy = by - ay;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared < 1e-9) {

    return Math.hypot(px - ax, py - ay);
  }

  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / lengthSquared));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}
