import { describe, expect, it } from "vitest";

import {
  distanceToSegment,
  IDENTITY,
  MAX_ZOOM,
  MIN_ZOOM,
  NODE_MAX_PX,
  NODE_MIN_PX,
  nodeScreenRadius,
  fitBox,
  pan,
  screenToWorld,
  trimmedBox,
  visibleWorldRect,
  worldToScreen,
  zoomAt,
} from "./viewport";

describe("world and screen coordinates", () => {
  it("round-trips at any transform", () => {
    const v = { x: 137, y: -42, zoom: 2.5 };
    for (const [wx, wy] of [
      [0, 0],
      [100, -250],
      [-1e4, 1e4],
    ] as const) {
      const [sx, sy] = worldToScreen(v, wx, wy);
      const [bx, by] = screenToWorld(v, sx, sy);
      expect(bx).toBeCloseTo(wx, 6);
      expect(by).toBeCloseTo(wy, 6);
    }
  });

  it("is the identity at the identity transform", () => {
    expect(worldToScreen(IDENTITY, 12, 34)).toEqual([12, 34]);
  });
});

describe("zoomAt", () => {
  it("keeps the world point under the cursor fixed", () => {

    const v = { x: 10, y: 20, zoom: 1 };
    const [cursorX, cursorY] = [300, 200];
    const before = screenToWorld(v, cursorX, cursorY);
    const after = screenToWorld(zoomAt(v, cursorX, cursorY, 2.3), cursorX, cursorY);
    expect(after[0]).toBeCloseTo(before[0], 6);
    expect(after[1]).toBeCloseTo(before[1], 6);
  });

  it("clamps rather than letting zoom run away", () => {
    let v = { x: 0, y: 0, zoom: 1 };
    for (let i = 0; i < 200; i++) v = zoomAt(v, 0, 0, 2);
    expect(v.zoom).toBe(MAX_ZOOM);
    for (let i = 0; i < 400; i++) v = zoomAt(v, 0, 0, 0.5);
    expect(v.zoom).toBe(MIN_ZOOM);
  });

  it("returns the same viewport when already clamped, so no needless redraw", () => {
    const v = { x: 5, y: 5, zoom: MAX_ZOOM };
    expect(zoomAt(v, 0, 0, 2)).toBe(v);
  });
});

describe("pan", () => {
  it("shifts the origin without touching zoom", () => {
    const v = pan({ x: 10, y: 10, zoom: 3 }, -4, 7);
    expect(v).toEqual({ x: 6, y: 17, zoom: 3 });
  });
});

describe("visibleWorldRect", () => {
  it("describes exactly what the canvas covers", () => {
    const v = { x: 0, y: 0, zoom: 2 };
    expect(visibleWorldRect(v, 800, 600)).toEqual({
      minX: 0,
      minY: 0,
      maxX: 400,
      maxY: 300,
    });
  });

  it("grows as you zoom out, which is what culling relies on", () => {
    const tight = visibleWorldRect({ x: 0, y: 0, zoom: 4 }, 800, 600);
    const wide = visibleWorldRect({ x: 0, y: 0, zoom: 0.5 }, 800, 600);
    expect(wide.maxX - wide.minX).toBeGreaterThan(tight.maxX - tight.minX);
  });
});

describe("fitBox", () => {
  it("centres the box in the viewport", () => {
    const box = { minX: -100, minY: -50, maxX: 100, maxY: 50 };
    const v = fitBox(box, 800, 600);
    const [cx, cy] = worldToScreen(v, 0, 0);
    expect(cx).toBeCloseTo(400, 3);
    expect(cy).toBeCloseTo(300, 3);
  });

  it("fits the whole box on screen with room to spare", () => {
    const box = { minX: -1000, minY: -800, maxX: 1000, maxY: 800 };
    const v = fitBox(box, 800, 600, 40);
    const [x0, y0] = worldToScreen(v, box.minX, box.minY);
    const [x1, y1] = worldToScreen(v, box.maxX, box.maxY);
    expect(x0).toBeGreaterThanOrEqual(0);
    expect(y0).toBeGreaterThanOrEqual(0);
    expect(x1).toBeLessThanOrEqual(800);
    expect(y1).toBeLessThanOrEqual(600);
  });

  it("survives a degenerate box without dividing by zero", () => {
    const v = fitBox({ minX: 5, minY: 5, maxX: 5, maxY: 5 }, 800, 600);
    expect(Number.isFinite(v.zoom)).toBe(true);
    expect(Number.isFinite(v.x)).toBe(true);
    expect(v.zoom).toBeLessThanOrEqual(MAX_ZOOM);
  });
});

describe("trimmedBox", () => {
  it("ignores a handful of far-flung outliers", () => {


    const pts: number[] = [];
    for (let i = 0; i < 100; i++) pts.push(i % 10, Math.floor(i / 10));
    pts.push(100000, 100000, -100000, -100000);

    const box = trimmedBox(pts, 0.02);
    expect(box.maxX).toBeLessThan(100);
    expect(box.minX).toBeGreaterThan(-100);
  });

  it("keeps everything when there is nothing to trim", () => {
    const pts = [0, 0, 10, 10, 5, 5];
    expect(trimmedBox(pts)).toEqual({ minX: 0, minY: 0, maxX: 10, maxY: 10 });
  });

  it("never trims away more than half the nodes", () => {
    const pts: number[] = [];
    for (let i = 0; i < 20; i++) pts.push(i, i);
    const box = trimmedBox(pts, 0.9);
    expect(box.maxX).toBeGreaterThanOrEqual(box.minX);
  });

  it("handles an empty input", () => {
    expect(trimmedBox([])).toEqual({ minX: 0, minY: 0, maxX: 1, maxY: 1 });
  });

  it("accepts a Float32Array, which is what the bridge delivers", () => {
    const box = trimmedBox(Float32Array.from([0, 0, 4, 8]));
    expect(box).toEqual({ minX: 0, minY: 0, maxX: 4, maxY: 8 });
  });
});

describe("nodeScreenRadius", () => {
  it("never falls below the clickable floor, however far out you zoom", () => {


    for (const zoom of [MIN_ZOOM, 0.05, 0.2, 1]) {
      expect(nodeScreenRadius(1, zoom)).toBeGreaterThanOrEqual(NODE_MIN_PX);
      expect(nodeScreenRadius(0.001, zoom)).toBeGreaterThanOrEqual(NODE_MIN_PX);
    }
  });

  it("never exceeds the ceiling, however far in you zoom", () => {
    for (const zoom of [4, 12, MAX_ZOOM]) {
      expect(nodeScreenRadius(40, zoom)).toBeLessThanOrEqual(NODE_MAX_PX);
    }
  });

  it("keeps larger files larger, which is what carries the LOC information", () => {
    const small = nodeScreenRadius(4, 1);
    const large = nodeScreenRadius(16, 1);
    expect(large).toBeGreaterThan(small);
  });

  it("grows with zoom, but more slowly than distance does", () => {


    const at1 = nodeScreenRadius(8, 1);
    const at4 = nodeScreenRadius(8, 4);
    expect(at4).toBeGreaterThan(at1);
    expect(at4).toBeLessThan(at1 * 4);
  });

  it("is monotonic in zoom", () => {
    let previous = 0;
    for (let z = 0.05; z <= 8; z += 0.05) {
      const r = nodeScreenRadius(6, z);
      expect(r).toBeGreaterThanOrEqual(previous - 1e-9);
      previous = r;
    }
  });

  it("survives a zero or negative radius without producing NaN", () => {
    expect(Number.isFinite(nodeScreenRadius(0, 1))).toBe(true);
    expect(Number.isFinite(nodeScreenRadius(5, 0))).toBe(true);
  });
});

describe("distanceToSegment", () => {
  it("is zero on the segment itself", () => {
    expect(distanceToSegment(5, 0, 0, 0, 10, 0)).toBeCloseTo(0);
  });

  it("measures perpendicular distance beside the segment", () => {
    expect(distanceToSegment(5, 3, 0, 0, 10, 0)).toBeCloseTo(3);
  });

  it("measures to the nearest end past the segment, not to the infinite line", () => {


    expect(distanceToSegment(20, 0, 0, 0, 10, 0)).toBeCloseTo(10);
    expect(distanceToSegment(-5, 0, 0, 0, 10, 0)).toBeCloseTo(5);
  });

  it("handles a zero-length segment as a point", () => {
    expect(distanceToSegment(3, 4, 0, 0, 0, 0)).toBeCloseTo(5);
  });

  it("is symmetric in the segment's endpoints", () => {
    const a = distanceToSegment(4, 7, 1, 2, 9, 3);
    const b = distanceToSegment(4, 7, 9, 3, 1, 2);
    expect(a).toBeCloseTo(b);
  });
});
