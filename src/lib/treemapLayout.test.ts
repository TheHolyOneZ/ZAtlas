import { describe, expect, it } from "vitest";

import {
  HEADER_HEIGHT,
  buildTree,
  cellAt,
  layoutTreemap,
  type TreeInput,
} from "./treemapLayout";

const files: TreeInput[] = [
  { id: 0, path: "src/a.ts", name: "a.ts", value: 100, heat: 0.1 },
  { id: 1, path: "src/b.ts", name: "b.ts", value: 200, heat: 0.5 },
  { id: 2, path: "src/ui/c.ts", name: "c.ts", value: 300, heat: 0.9 },
];

describe("buildTree", () => {
  it("nests files under their directories", () => {
    const root = buildTree(files, "repo");
    const src = root.children.find((c) => c.name === "src");
    expect(src).toBeDefined();
    expect(src!.children.map((c) => c.name).sort()).toEqual(["a.ts", "b.ts", "ui"]);
  });

  it("gives every leaf a non-zero value, so nothing vanishes", () => {
    const root = buildTree(
      [{ id: 0, path: "empty.ts", name: "empty.ts", value: 0, heat: 0 }],
      "repo",
    );
    expect(root.children[0]!.value).toBeGreaterThan(0);
  });

  it("keeps two files with the same name in different directories apart", () => {
    const root = buildTree(
      [
        { id: 0, path: "a/index.ts", name: "index.ts", value: 10, heat: 0 },
        { id: 1, path: "b/index.ts", name: "index.ts", value: 10, heat: 0 },
      ],
      "repo",
    );
    expect(root.children.map((c) => c.name).sort()).toEqual(["a", "b"]);
  });
});

describe("layoutTreemap", () => {
  it("returns a cell for every file", () => {
    const cells = layoutTreemap(files, 800, 600);
    const leaves = cells.filter((c) => c.isLeaf);
    expect(leaves.map((c) => c.path).sort()).toEqual([
      "src/a.ts",
      "src/b.ts",
      "src/ui/c.ts",
    ]);
  });

  it("keeps every cell inside the canvas", () => {
    for (const c of layoutTreemap(files, 800, 600)) {
      expect(c.x0).toBeGreaterThanOrEqual(0);
      expect(c.y0).toBeGreaterThanOrEqual(0);
      expect(c.x1).toBeLessThanOrEqual(800);
      expect(c.y1).toBeLessThanOrEqual(600);
    }
  });

  it("gives a bigger file more area", () => {
    const cells = layoutTreemap(files, 800, 600).filter((c) => c.isLeaf);
    const area = (p: string) => {
      const c = cells.find((x) => x.path === p)!;
      return (c.x1 - c.x0) * (c.y1 - c.y0);
    };
    expect(area("src/ui/c.ts")).toBeGreaterThan(area("src/a.ts"));
  });

  it("drops sub-two-pixel cells, which cannot be seen or clicked anyway", () => {
    const many: TreeInput[] = Array.from({ length: 5000 }, (_, i) => ({
      id: i,
      path: `src/f${i}.ts`,
      name: `f${i}.ts`,
      value: 1,
      heat: 0,
    }));
    const cells = layoutTreemap(many, 300, 200);
    expect(cells.length).toBeLessThan(5000);
    for (const c of cells) {
      expect(c.x1 - c.x0).toBeGreaterThanOrEqual(2);
      expect(c.y1 - c.y0).toBeGreaterThanOrEqual(2);
    }
  });

  it("returns nothing for an empty input or a zero-sized canvas", () => {
    expect(layoutTreemap([], 800, 600)).toEqual([]);
    expect(layoutTreemap(files, 0, 600)).toEqual([]);
  });

  it("is deterministic", () => {
    expect(layoutTreemap(files, 800, 600)).toEqual(layoutTreemap(files, 800, 600));
  });
});

describe("cellAt", () => {
  it("returns the innermost cell under a point", () => {
    const cells = layoutTreemap(files, 800, 600);
    const leaf = cells.find((c) => c.isLeaf)!;
    const hit = cellAt(cells, (leaf.x0 + leaf.x1) / 2, (leaf.y0 + leaf.y1) / 2);
    expect(hit?.depth).toBeGreaterThanOrEqual(leaf.depth);
    expect(hit?.isLeaf).toBe(true);
  });

  it("returns null outside every cell", () => {
    expect(cellAt(layoutTreemap(files, 800, 600), -50, -50)).toBeNull();
  });
});

describe("directory header strips", () => {
  it("reserves a full header above every directory's children", () => {


    const nested: TreeInput[] = [
      { id: 0, path: "repo/src/a.ts", name: "a.ts", value: 400, heat: 0 },
      { id: 1, path: "repo/src/b.ts", name: "b.ts", value: 300, heat: 0 },
      { id: 2, path: "repo/lib/c.ts", name: "c.ts", value: 200, heat: 0 },
    ];
    const cells = layoutTreemap(nested, 900, 700);

    for (const parent of cells.filter((c) => !c.isLeaf)) {
      const children = cells.filter(
        (c) =>
          c !== parent &&
          c.path.startsWith(`${parent.path}/`) &&
          c.depth === parent.depth + 1,
      );
      for (const child of children) {
        expect(child.y0).toBeGreaterThanOrEqual(parent.y0 + HEADER_HEIGHT);
      }
    }
  });

  it("keeps every child strictly inside its parent", () => {
    const nested: TreeInput[] = [
      { id: 0, path: "a/b/one.ts", name: "one.ts", value: 100, heat: 0 },
      { id: 1, path: "a/b/two.ts", name: "two.ts", value: 100, heat: 0 },
      { id: 2, path: "a/c/three.ts", name: "three.ts", value: 100, heat: 0 },
    ];
    const cells = layoutTreemap(nested, 800, 600);
    for (const child of cells) {
      const parent = cells.find(
        (c) => c.depth === child.depth - 1 && child.path.startsWith(`${c.path}/`),
      );
      if (!parent) continue;
      expect(child.x0).toBeGreaterThanOrEqual(parent.x0);
      expect(child.x1).toBeLessThanOrEqual(parent.x1);
      expect(child.y1).toBeLessThanOrEqual(parent.y1);
    }
  });
});
