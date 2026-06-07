import { useEffect } from "react";

/**
 * Toggle the palette with ⌘K / Ctrl+K. Registered once, cleaned up on unmount.
 */
export function useGlobalHotkey(onToggle: () => void, key = "k") {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === key) {
        e.preventDefault();
        onToggle();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onToggle, key]);
}
