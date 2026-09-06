import { describe, expect, it } from "vitest";

import {
  heatGlyph,
  severityCss,
  severityHeat,
  severityOklch,
  severityRgb,
} from "./severity";

describe("the severity ramp", () => {
  it("runs from teal to red as heat rises", () => {
    expect(severityOklch(0).h).toBeCloseTo(180, 0);
    expect(severityOklch(1).h).toBeCloseTo(30, 0);
  });

  it("is monotonic in hue, so equal steps read as equal jumps in danger", () => {
    let previous = Infinity;
    for (let heat = 0; heat <= 1.0001; heat += 0.05) {
      const { h } = severityOklch(heat);
      expect(h).toBeLessThanOrEqual(previous + 1e-6);
      previous = h;
    }
  });

  it("clamps out-of-range heat rather than extrapolating off the ramp", () => {
    expect(severityOklch(-5)).toEqual(severityOklch(0));
    expect(severityOklch(99)).toEqual(severityOklch(1));
  });

  it("produces in-gamut sRGB at every point", () => {
    for (let heat = 0; heat <= 1.0001; heat += 0.02) {
      for (const light of [false, true]) {
        const rgb = severityRgb(heat, light);
        for (const channel of rgb) {
          expect(Number.isFinite(channel)).toBe(true);
          expect(channel).toBeGreaterThanOrEqual(0);
          expect(channel).toBeLessThanOrEqual(255);
        }
      }
    }
  });

  it("emits a canvas-parseable colour, since canvas cannot read oklch()", () => {
    expect(severityCss(0.5)).toMatch(/^rgb\(\d+,\d+,\d+\)$/);
  });

  it("keeps the light ramp darker than the dark one, for contrast on cream", () => {
    for (let heat = 0; heat <= 1.0001; heat += 0.25) {
      expect(severityOklch(heat, true).l).toBeLessThan(severityOklch(heat, false).l);
    }
  });
});

describe("secondary encoding", () => {
  it("pairs each band with a distinct glyph so colour is never alone", () => {
    expect(heatGlyph(0.1)).toBe("○");
    expect(heatGlyph(0.6)).toBe("●");
    expect(heatGlyph(0.9)).toBe("◉");
    expect(new Set([heatGlyph(0.1), heatGlyph(0.6), heatGlyph(0.9)]).size).toBe(3);
  });

  it("maps severity names onto the ramp", () => {
    expect(severityHeat("info")).toBe(0);
    expect(severityHeat("critical")).toBe(1);
    expect(severityHeat("medium")).toBeCloseTo(0.5);
    expect(severityHeat("nonsense")).toBe(0);
  });
});
