import { useEffect, useRef } from "react";

import { bindingFor, isTypingTarget } from "../lib/shortcuts";


export function useKeyboard(handlers: Record<string, () => void>, enabled = true) {
  const ref = useRef(handlers);
  ref.current = handlers;
  const on = useRef(enabled);
  on.current = enabled;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!on.current) return;
      if (isTypingTarget(e.target)) return;
      const handler = ref.current[bindingFor(e)];
      if (!handler) return;
      e.preventDefault();
      handler();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
