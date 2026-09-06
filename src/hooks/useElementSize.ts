import { useCallback, useRef, useState } from "react";

export interface Size {
  w: number;
  h: number;
}


export function observeSize(
  node: HTMLElement,
  onSize: (size: Size) => void,
): () => void {
  const measure = () => onSize({ w: node.clientWidth, h: node.clientHeight });
  const observer = new ResizeObserver(measure);
  observer.observe(node);
  measure();
  return () => observer.disconnect();
}


export function useElementSize<T extends HTMLElement>() {
  const [size, setSize] = useState<Size>({ w: 0, h: 0 });
  const disposeRef = useRef<(() => void) | null>(null);
  const nodeRef = useRef<T | null>(null);

  const ref = useCallback((node: T | null) => {
    disposeRef.current?.();
    disposeRef.current = null;
    nodeRef.current = node;
    if (!node) return;
    disposeRef.current = observeSize(node, (next) =>

      setSize((prev) => (prev.w === next.w && prev.h === next.h ? prev : next)),
    );
  }, []);

  return { ref, size, node: nodeRef } as const;
}
