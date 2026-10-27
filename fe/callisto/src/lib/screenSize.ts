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

/** How many console columns: Auto (by the screen's width), or 1, 2 or 3. */
export type ConsoleColumns = "auto" | "1" | "2" | "3";

const COLUMNS_KEY = "console-columns";

export function savedColumns(): ConsoleColumns {
  try {
    const value = window.localStorage.getItem(COLUMNS_KEY);
    return value === "1" || value === "2" || value === "3" ? value : "auto";
  } catch {
    return "auto";
  }
}

export function applyColumns(columns: ConsoleColumns) {
  if (columns === "auto") {
    delete document.documentElement.dataset.columns;
  } else {
    document.documentElement.dataset.columns = columns;
  }
  try {
    window.localStorage.setItem(COLUMNS_KEY, columns);
  } catch {
    // Not remembered; it still applies now.
  }
}
