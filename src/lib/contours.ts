

export interface ContourBand {

  level: number;
  segments: Float32Array;
}

export interface ContourOptions {

  resolution: number;

  spread: number;


  levels: number[];
}

export const DEFAULT_CONTOURS: ContourOptions = {


  resolution: 128,
  spread: 0.055,
  levels: [0.35, 0.55, 0.72, 0.85, 0.94],
};


export function buildContours(
  xs: Float32Array,
  ys: Float32Array,
  bounds: { minX: number; minY: number; maxX: number; maxY: number },
  opts: ContourOptions = DEFAULT_CONTOURS,
): ContourBand[] {
  const count = Math.min(xs.length, ys.length);
  if (count === 0) return [];

  const width = Math.max(bounds.maxX - bounds.minX, 1e-6);
  const height = Math.max(bounds.maxY - bounds.minY, 1e-6);
  const long = Math.max(width, height);


  const cell = long / opts.resolution;
  const cols = Math.max(2, Math.ceil(width / cell) + 1);
  const rows = Math.max(2, Math.ceil(height / cell) + 1);

  const field = new Float32Array(cols * rows);
  const radiusCells = Math.max(2, Math.round((opts.spread * long) / cell));


  const sigma = radiusCells / 2;
  const twoSigmaSq = 2 * sigma * sigma;

  for (let n = 0; n < count; n++) {
    const gx = (xs[n]! - bounds.minX) / cell;
    const gy = (ys[n]! - bounds.minY) / cell;
    const i0 = Math.max(0, Math.floor(gx - radiusCells));
    const i1 = Math.min(cols - 1, Math.ceil(gx + radiusCells));
    const j0 = Math.max(0, Math.floor(gy - radiusCells));
    const j1 = Math.min(rows - 1, Math.ceil(gy + radiusCells));

    for (let j = j0; j <= j1; j++) {
      const dy = j - gy;
      for (let i = i0; i <= i1; i++) {
        const dx = i - gx;
        const d2 = dx * dx + dy * dy;
        if (d2 > radiusCells * radiusCells) continue;
        field[j * cols + i]! += Math.exp(-d2 / twoSigmaSq);
      }
    }
  }


  const present: number[] = [];
  for (let i = 0; i < field.length; i++) {
    if (field[i]! > 1e-4) present.push(field[i]!);
  }
  if (present.length === 0) return [];
  present.sort((a, b) => a - b);

  return opts.levels.map((level) => {
    const at = present[Math.min(present.length - 1, Math.floor(level * present.length))]!;
    return {
      level,
      segments: marchingSquares(field, cols, rows, at, bounds.minX, bounds.minY, cell),
    };
  });
}


export function marchingSquares(
  field: Float32Array,
  cols: number,
  rows: number,
  threshold: number,
  originX: number,
  originY: number,
  cell: number,
): Float32Array {
  const out: number[] = [];

  const lerp = (a: number, b: number) => {
    const span = b - a;

    if (Math.abs(span) < 1e-9) return 0.5;
    return Math.min(1, Math.max(0, (threshold - a) / span));
  };

  for (let j = 0; j < rows - 1; j++) {
    for (let i = 0; i < cols - 1; i++) {
      const tl = field[j * cols + i]!;
      const tr = field[j * cols + i + 1]!;
      const br = field[(j + 1) * cols + i + 1]!;
      const bl = field[(j + 1) * cols + i]!;

      let index = 0;
      if (tl >= threshold) index |= 8;
      if (tr >= threshold) index |= 4;
      if (br >= threshold) index |= 2;
      if (bl >= threshold) index |= 1;
      if (index === 0 || index === 15) continue;

      const x0 = originX + i * cell;
      const y0 = originY + j * cell;
      const x1 = x0 + cell;
      const y1 = y0 + cell;

      const top: [number, number] = [x0 + lerp(tl, tr) * cell, y0];
      const right: [number, number] = [x1, y0 + lerp(tr, br) * cell];
      const bottom: [number, number] = [x0 + lerp(bl, br) * cell, y1];
      const left: [number, number] = [x0, y0 + lerp(tl, bl) * cell];

      const push = (a: [number, number], b: [number, number]) => {
        out.push(a[0], a[1], b[0], b[1]);
      };

      switch (index) {
        case 1:
        case 14:
          push(left, bottom);
          break;
        case 2:
        case 13:
          push(bottom, right);
          break;
        case 3:
        case 12:
          push(left, right);
          break;
        case 4:
        case 11:
          push(top, right);
          break;
        case 6:
        case 9:
          push(top, bottom);
          break;
        case 7:
        case 8:
          push(left, top);
          break;
        case 5: {


          const centre = (tl + tr + br + bl) / 4;
          if (centre >= threshold) {
            push(left, top);
            push(bottom, right);
          } else {
            push(left, bottom);
            push(top, right);
          }
          break;
        }
        case 10: {
          const centre = (tl + tr + br + bl) / 4;
          if (centre >= threshold) {
            push(left, bottom);
            push(top, right);
          } else {
            push(left, top);
            push(bottom, right);
          }
          break;
        }
        default:
          break;
      }
    }
  }

  return Float32Array.from(out);
}
