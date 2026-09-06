import { describe, expect, it } from "vitest";

import { buildContours, marchingSquares, DEFAULT_CONTOURS } from "./contours";


function points(segments: Float32Array): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i < segments.length; i += 2) {
    out.push([segments[i]!, segments[i + 1]!]);
  }
  return out;
}

describe("marchingSquares", () => {
  it("finds nothing in a field that is entirely below the threshold", () => {
    const field = new Float32Array([0, 0, 0, 0]);
    expect(marchingSquares(field, 2, 2, 0.5, 0, 0, 1).length).toBe(0);
  });

  it("finds nothing in a field that is entirely above it", () => {


    const field = new Float32Array([1, 1, 1, 1]);
    expect(marchingSquares(field, 2, 2, 0.5, 0, 0, 1).length).toBe(0);
  });

  it("cuts a single cell in half when one corner is above", () => {

    const field = new Float32Array([1, 0, 0, 0]);
    const segments = marchingSquares(field, 2, 2, 0.5, 0, 0, 1);
    expect(segments.length).toBe(4);
    const [a, b] = points(segments);

    expect(Math.min(a![0], b![0])).toBe(0);
    expect(Math.min(a![1], b![1])).toBe(0);
  });

  it("interpolates the crossing rather than using the cell midpoint", () => {


    const field = new Float32Array([1, 0, 0, 0]);
    const segments = marchingSquares(field, 2, 2, 0.25, 0, 0, 1);
    const xs = points(segments).map((p) => p[0]);
    expect(Math.max(...xs)).toBeCloseTo(0.75, 5);
  });

  it("does not divide by zero when two corners are exactly equal", () => {
    const field = new Float32Array([0.5, 0.5, 0, 0]);
    const segments = marchingSquares(field, 2, 2, 0.5, 0, 0, 1);
    for (const value of segments) expect(Number.isFinite(value)).toBe(true);
  });

  it("resolves the saddle case into two separate lines", () => {


    const field = new Float32Array([1, 0, 0, 1]);
    const segments = marchingSquares(field, 2, 2, 0.5, 0, 0, 1);
    expect(segments.length).toBe(8);
  });

  it("places lines in world coordinates, not grid indices", () => {
    const field = new Float32Array([1, 0, 0, 0]);
    const segments = marchingSquares(field, 2, 2, 0.5, 100, 200, 10);
    for (const [x, y] of points(segments)) {
      expect(x).toBeGreaterThanOrEqual(100);
      expect(y).toBeGreaterThanOrEqual(200);
      expect(x).toBeLessThanOrEqual(110);
      expect(y).toBeLessThanOrEqual(210);
    }
  });
});

describe("buildContours", () => {
  const bounds = { minX: -100, minY: -100, maxX: 100, maxY: 100 };

  it("returns nothing for an empty graph", () => {
    expect(buildContours(new Float32Array(), new Float32Array(), bounds)).toEqual([]);
  });

  it("draws rings around a cluster rather than around the origin", () => {


    const n = 40;
    const xs = new Float32Array(n).fill(60);
    const ys = new Float32Array(n).fill(60);
    const bands = buildContours(xs, ys, bounds);
    const all = bands.flatMap((b) => points(b.segments));
    expect(all.length).toBeGreaterThan(0);
    for (const [x, y] of all) {
      expect(x).toBeGreaterThan(0);
      expect(y).toBeGreaterThan(0);
    }
  });

  it("gives two clusters their own sets of rings", () => {
    const xs = new Float32Array([-60, -60, -60, 60, 60, 60]);
    const ys = new Float32Array([-60, -60, -60, 60, 60, 60]);
    const bands = buildContours(xs, ys, bounds);
    const inner = bands[bands.length - 1]!;
    const pts = points(inner.segments);
    expect(pts.some(([x]) => x < 0)).toBe(true);
    expect(pts.some(([x]) => x > 0)).toBe(true);
  });

  it("puts lines where the ground changes even when one cluster dwarfs the rest", () => {


    const dense = 80;
    const xs = new Float32Array(dense + 3);
    const ys = new Float32Array(dense + 3);
    for (let i = 0; i < dense; i++) {
      xs[i] = 60;
      ys[i] = 60;
    }
    xs[dense] = -60;
    ys[dense] = -60;
    xs[dense + 1] = -62;
    ys[dense + 1] = -58;
    xs[dense + 2] = -58;
    ys[dense + 2] = -62;

    const bands = buildContours(xs, ys, bounds);
    const outer = points(bands[0]!.segments);
    expect(outer.some(([x, y]) => x < 0 && y < 0)).toBe(true);
  });

  it("orders bands from the outermost level to the innermost", () => {
    const bands = buildContours(
      new Float32Array([0, 5]),
      new Float32Array([0, 5]),
      bounds,
    );
    const levels = bands.map((b) => b.level);
    expect(levels).toEqual([...levels].sort((a, b) => a - b));
    expect(levels).toEqual(DEFAULT_CONTOURS.levels);
  });

  it("keeps cells square so a wide graph does not get oval contours", () => {


    const wide = { minX: -200, minY: -50, maxX: 200, maxY: 50 };
    const n = 60;
    const xs = new Float32Array(n);
    const ys = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const a = (i / n) * Math.PI * 2;
      xs[i] = Math.cos(a) * 8;
      ys[i] = Math.sin(a) * 8;
    }
    const band = buildContours(xs, ys, wide)[0]!;
    const pts = points(band.segments);
    const spanX = Math.max(...pts.map((p) => p[0])) - Math.min(...pts.map((p) => p[0]));
    const spanY = Math.max(...pts.map((p) => p[1])) - Math.min(...pts.map((p) => p[1]));
    expect(spanX / spanY).toBeGreaterThan(0.75);
    expect(spanX / spanY).toBeLessThan(1.35);
  });

  it("survives every node sitting on exactly the same point", () => {
    const xs = new Float32Array(10).fill(0);
    const ys = new Float32Array(10).fill(0);
    const bands = buildContours(xs, ys, bounds);
    for (const band of bands) {
      for (const value of band.segments) expect(Number.isFinite(value)).toBe(true);
    }
  });

  it("survives a degenerate bounding box", () => {

    const flat = { minX: 5, minY: 5, maxX: 5, maxY: 5 };
    const bands = buildContours(new Float32Array([5]), new Float32Array([5]), flat);
    for (const band of bands) {
      for (const value of band.segments) expect(Number.isFinite(value)).toBe(true);
    }
  });
});
