import { describe, expect, it } from "vitest";

import {
  escapeXml,
  projectGraph,
  renderSvg,
  substituteVars,
  type SvgTheme,
} from "./graphSvg";
import { IDENTITY } from "./viewport";

const THEME: SvgTheme = {
  background: "#0B1211",
  edge: "#43a99b",
  weak: "#f87171",
  text: "#93a5a1",
  halo: "#0B1211",
};

describe("escapeXml", () => {
  it("escapes the five characters that would break the document", () => {
    expect(escapeXml(`a<b>&"'`)).toBe("a&lt;b&gt;&amp;&quot;&apos;");
  });

  it("escapes a path that looks like markup", () => {


    expect(escapeXml("src/<weird>.ts")).toBe("src/&lt;weird&gt;.ts");
  });
});

describe("renderSvg", () => {
  const base = { width: 100, height: 80, theme: THEME };

  it("produces a well-formed document with the right dimensions", () => {
    const svg = renderSvg([], [], base);
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg.trimEnd().endsWith("</svg>")).toBe(true);
    expect(svg).toContain('width="100"');
    expect(svg).toContain('viewBox="0 0 100 80"');
  });

  it("paints an explicit background rather than relying on the viewer", () => {


    expect(renderSvg([], [], base)).toContain('fill="#0B1211"');
  });

  it("batches edges into one path per kind instead of an element each", () => {
    const edges = Array.from({ length: 50 }, (_, i) => ({
      x1: i,
      y1: 0,
      x2: i,
      y2: 10,
      weak: false,
    }));
    const svg = renderSvg([], edges, base);
    expect(svg.match(/<path /g)?.length).toBe(1);
    expect(svg.match(/<line /g)).toBeNull();
  });

  it("separates weak edges so they can be dashed", () => {
    const svg = renderSvg(
      [],
      [
        { x1: 0, y1: 0, x2: 1, y2: 1, weak: false },
        { x1: 2, y1: 2, x2: 3, y2: 3, weak: true },
      ],
      base,
    );
    expect(svg.match(/<path /g)?.length).toBe(2);
    expect(svg).toContain("stroke-dasharray");
  });

  it("omits the edge groups entirely when there are none", () => {
    expect(renderSvg([{ x: 1, y: 1, r: 2, fill: "#fff" }], [], base)).not.toContain("<path");
  });

  it("gives labels a halo so they read over edges", () => {
    const svg = renderSvg([{ x: 5, y: 5, r: 2, fill: "#fff", label: "a.ts" }], [], base);
    expect(svg).toContain("paint-order=\"stroke\"");
    expect(svg).toContain(">a.ts</text>");
  });

  it("rounds coordinates so a large graph does not become megabytes", () => {
    const svg = renderSvg([{ x: 1.23456789, y: 2.3456789, r: 1, fill: "#fff" }], [], base);
    expect(svg).toContain('cx="1.23"');
    expect(svg).not.toContain("1.23456789");
  });

  it("survives a non-finite coordinate instead of writing NaN into the file", () => {
    const svg = renderSvg([{ x: NaN, y: 5, r: 2, fill: "#fff" }], [], base);
    expect(svg).not.toContain("NaN");
  });
});

describe("projectGraph", () => {
  const positions = Float32Array.from([0, 0, 10, 10, 20, 20]);
  const radii = Float32Array.from([4, 4, 4]);
  const all = () => true;
  const teal = () => "#2dd4bf";
  const noLabel = () => undefined;

  it("drops edges whose endpoints were filtered out", () => {

    const { nodes, edges } = projectGraph(
      positions,
      radii,
      [
        { from: 0, to: 1, kind: "import" },
        { from: 1, to: 2, kind: "import" },
      ],
      IDENTITY,
      teal,
      (i) => i !== 2,
      noLabel,
    );
    expect(nodes.length).toBe(2);
    expect(edges.length).toBe(1);
  });

  it("leaves co-change edges out of a structural diagram", () => {
    const { edges } = projectGraph(
      positions,
      radii,
      [{ from: 0, to: 1, kind: "coChange" }],
      IDENTITY,
      teal,
      all,
      noLabel,
    );
    expect(edges).toEqual([]);
  });

  it("marks a barrel edge weak, matching what the canvas draws dashed", () => {


    const { edges } = projectGraph(
      positions,
      radii,
      [
        { from: 0, to: 1, kind: "viaBarrel" },
        { from: 1, to: 2, kind: "import" },
      ],
      IDENTITY,
      teal,
      all,
      noLabel,
    );
    expect(edges[0]!.weak).toBe(true);
    expect(edges[1]!.weak).toBe(false);
  });

  it("never emits a node too small to see", () => {
    const { nodes } = projectGraph(
      positions,
      Float32Array.from([0, 0, 0]),
      [],
      IDENTITY,
      teal,
      all,
      noLabel,
    );
    for (const node of nodes) expect(node.r).toBeGreaterThanOrEqual(1.5);
  });
});

describe("substituteVars", () => {
  const resolve = (name: string) => (name === "--accent" ? "#2dd4bf" : "");

  it("replaces a custom property with its resolved value", () => {
    expect(substituteVars("var(--accent)", resolve)).toBe("#2dd4bf");
  });

  it("replaces one inside a larger value", () => {
    expect(substituteVars("1px solid var(--accent)", resolve)).toBe("1px solid #2dd4bf");
  });

  it("uses the declared fallback when the property resolves to nothing", () => {
    expect(substituteVars("var(--missing, #f00)", resolve)).toBe("#f00");
  });

  it("falls back to `none` rather than leaving an empty attribute", () => {


    expect(substituteVars("var(--missing)", resolve)).toBe("none");
  });

  it("handles several in one value", () => {
    expect(substituteVars("var(--accent) var(--accent)", resolve)).toBe("#2dd4bf #2dd4bf");
  });

  it("leaves a value with no custom properties alone", () => {
    expect(substituteVars("#123456", resolve)).toBe("#123456");
  });

  it("tolerates whitespace inside the parentheses", () => {
    expect(substituteVars("var( --accent )", resolve)).toBe("#2dd4bf");
  });
});
