/**
 * How big the interface is drawn: Auto (compact on a laptop-sized screen,
 * full on a big one), or always Compact, or always Full. Stamped on the root
 * element as data-size, which the stylesheet reads; remembered in this
 * browser.
 */
export type ScreenSize = "auto" | "compact" | "full";

const KEY = "screen-size";

export function savedScreenSize(): ScreenSize {
  try {
    const value = window.localStorage.getItem(KEY);
    return value === "compact" || value === "full" ? value : "auto";
  } catch {
    return "auto";
  }
}

export function applyScreenSize(size: ScreenSize) {
  if (size === "auto") {
    delete document.documentElement.dataset.size;
  } else {
    document.documentElement.dataset.size = size;
  }
  try {
    window.localStorage.setItem(KEY, size);
  } catch {
    // Not remembered; it still applies now.
  }
}
