import { create } from "zustand";


export interface ViewCapture {

  view: string;

  toSvg: (() => string) | null;

  canvas: () => HTMLCanvasElement | null;
}

interface CaptureState {
  capture: ViewCapture | null;
  setCapture: (capture: ViewCapture | null) => void;
}

export const useCaptureStore = create<CaptureState>((set) => ({
  capture: null,
  setCapture: (capture) => set({ capture }),
}));


export function canvasToPng(canvas: HTMLCanvasElement, scale = 2): string {
  if (scale <= 1) return canvas.toDataURL("image/png");
  const out = document.createElement("canvas");
  out.width = canvas.width * scale;
  out.height = canvas.height * scale;
  const ctx = out.getContext("2d");
  if (!ctx) return canvas.toDataURL("image/png");


  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(canvas, 0, 0, out.width, out.height);
  return out.toDataURL("image/png");
}
