import { hierarchy, treemap, treemapSquarify } from "d3-hierarchy";


export interface TreeInput {
  id: number;
  path: string;
  name: string;
  value: number;
  heat: number;
}


export const HEADER_HEIGHT = 14;

export interface Cell {
  id: number;
  path: string;
  name: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  depth: number;
  heat: number;
  isLeaf: boolean;
}

interface Node {
  name: string;
  path: string;
  id: number;
  value: number;
  heat: number;
  children: Node[];
}


export function buildTree(files: TreeInput[], rootName: string): Node {
  const root: Node = { name: rootName, path: "", id: -1, value: 0, heat: 0, children: [] };

  for (const f of files) {
    const parts = f.path.split("/");
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const segment = parts[i]!;
      const childPath = parts.slice(0, i + 1).join("/");
      let next = node.children.find((c) => c.name === segment && c.id === -1);
      if (!next) {
        next = { name: segment, path: childPath, id: -1, value: 0, heat: 0, children: [] };
        node.children.push(next);
      }
      node = next;
    }
    node.children.push({
      name: parts[parts.length - 1]!,
      path: f.path,
      id: f.id,
      value: Math.max(f.value, 1),
      heat: f.heat,
      children: [],
    });
  }
  return root;
}


export function layoutTreemap(
  files: TreeInput[],
  width: number,
  height: number,
  rootName = "root",
): Cell[] {
  if (files.length === 0 || width <= 0 || height <= 0) return [];

  const root = hierarchy<Node>(buildTree(files, rootName), (d) =>
    d.children.length > 0 ? d.children : null,
  )
    .sum((d) => (d.children.length === 0 ? d.value : 0))
    .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

  treemap<Node>()
    .size([width, height])


    .paddingOuter(2)
    .paddingInner(1)
    .paddingTop(HEADER_HEIGHT)
    .tile(treemapSquarify)(root);

  const cells: Cell[] = [];
  root.each((node) => {
    const n = node as unknown as {
      x0: number;
      y0: number;
      x1: number;
      y1: number;
      depth: number;
      data: Node;
    };
    const w = n.x1 - n.x0;
    const h = n.y1 - n.y0;
    if (w < 2 || h < 2) return;
    if (n.depth === 0) return;
    cells.push({
      id: n.data.id,
      path: n.data.path,
      name: n.data.name,
      x0: n.x0,
      y0: n.y0,
      x1: n.x1,
      y1: n.y1,
      depth: n.depth,
      heat: n.data.heat,
      isLeaf: n.data.children.length === 0,
    });
  });
  return cells;
}


export function cellAt(cells: Cell[], x: number, y: number): Cell | null {
  let best: Cell | null = null;
  for (const c of cells) {
    if (x >= c.x0 && x <= c.x1 && y >= c.y0 && y <= c.y1) {
      if (!best || c.depth > best.depth) best = c;
    }
  }
  return best;
}
