

interface Oklch {
  l: number;
  c: number;
  h: number;
}


const STOPS: Oklch[] = [
  { l: 0.78, c: 0.13, h: 180 },
  { l: 0.8, c: 0.14, h: 140 },
  { l: 0.83, c: 0.16, h: 95 },
  { l: 0.76, c: 0.18, h: 60 },
  { l: 0.66, c: 0.21, h: 30 },
];

const LIGHT_STOPS: Oklch[] = [
  { l: 0.62, c: 0.11, h: 180 },
  { l: 0.63, c: 0.13, h: 140 },
  { l: 0.66, c: 0.15, h: 95 },
  { l: 0.6, c: 0.18, h: 60 },
  { l: 0.52, c: 0.2, h: 30 },
];

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}


export function severityOklch(heat: number, light = false): Oklch {
  const stops = light ? LIGHT_STOPS : STOPS;
  const clamped = Math.min(1, Math.max(0, heat));
  const scaled = clamped * (stops.length - 1);
  const i = Math.min(stops.length - 2, Math.floor(scaled));
  const t = scaled - i;
  const a = stops[i]!;
  const b = stops[i + 1]!;
  return {
    l: lerp(a.l, b.l, t),
    c: lerp(a.c, b.c, t),


    h: lerp(a.h, b.h, t),
  };
}

export function severityColor(heat: number, light = false): string {
  const { l, c, h } = severityOklch(heat, light);
  return `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)})`;
}


export function severityRgb(heat: number, light = false): [number, number, number] {
  const { l, c, h } = severityOklch(heat, light);
  const hr = (h * Math.PI) / 180;
  const a = c * Math.cos(hr);
  const bb = c * Math.sin(hr);

  const l_ = l + 0.3963377774 * a + 0.2158037573 * bb;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * bb;
  const s_ = l - 0.0894841775 * a - 1.291485548 * bb;

  const lc = l_ * l_ * l_;
  const mc = m_ * m_ * m_;
  const sc = s_ * s_ * s_;

  const rLin = 4.0767416621 * lc - 3.3077115913 * mc + 0.2309699292 * sc;
  const gLin = -1.2684380046 * lc + 2.6097574011 * mc - 0.3413193965 * sc;
  const bLin = -0.0041960863 * lc - 0.7034186147 * mc + 1.707614701 * sc;

  const gamma = (v: number) => {
    const x = v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(Math.max(v, 0), 1 / 2.4) - 0.055;
    return Math.round(Math.min(1, Math.max(0, x)) * 255);
  };
  return [gamma(rLin), gamma(gLin), gamma(bLin)];
}

export function severityCss(heat: number, light = false): string {
  const [r, g, b] = severityRgb(heat, light);
  return `rgb(${r},${g},${b})`;
}


export function heatGlyph(heat: number): string {
  if (heat >= 0.8) return "◉";
  if (heat >= 0.5) return "●";
  return "○";
}

export const SEVERITY_LABELS = ["info", "low", "medium", "high", "critical"] as const;

export function severityHeat(severity: string): number {
  const i = SEVERITY_LABELS.indexOf(severity as (typeof SEVERITY_LABELS)[number]);
  return i < 0 ? 0 : i / (SEVERITY_LABELS.length - 1);
}
