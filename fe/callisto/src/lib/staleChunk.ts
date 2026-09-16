/**
 * Recover from a deploy pulling a chunk out from under an open tab.
 *
 * Vite names every chunk by content hash, and the entry script a tab loaded
 * earlier still asks for the names it was built with. After a deploy those
 * files are gone, so the first lazy import the tab needs -- opening Results,
 * say -- fetches a 404, throws "Failed to fetch dynamically imported module",
 * and lands in the error boundary as "Something went wrong". A refresh fixes
 * it, because the refresh gets the new entry with the new names. Every open
 * tab hits this on every deploy.
 *
 * Vite raises `vite:preloadError` for exactly this case. Reloading is the
 * right response, but only once: if the chunk is genuinely missing -- a broken
 * build -- reloading in a loop would be worse than the error. So a reload is
 * allowed at most once per window, tracked in sessionStorage, and otherwise the
 * error is left to propagate to the boundary as before.
 */

const RELOAD_KEY = "callisto:chunk-reload-at";

/** Minimum time between automatic reloads for this tab. */
export const RELOAD_WINDOW_MS = 60_000;

/**
 * Whether to reload now, given when this tab last did so. Pure, so the guard
 * against looping can be tested without a browser.
 */
export const shouldReload = (lastReloadAt: number | null, now: number): boolean =>
  lastReloadAt == null || now - lastReloadAt > RELOAD_WINDOW_MS;

const readLastReload = (): number | null => {
  try {
    const raw = sessionStorage.getItem(RELOAD_KEY);
    return raw == null ? null : Number(raw);
  } catch {
    // Private windows and blocked storage: behave as if never reloaded, which
    // still bounds us to one reload per page load.
    return null;
  }
};

const recordReload = (now: number) => {
  try {
    sessionStorage.setItem(RELOAD_KEY, String(now));
  } catch {
    // Nothing to do; the page is about to reload regardless.
  }
};

/** Install once, before the app renders. */
export const installStaleChunkReload = (): void => {
  window.addEventListener("vite:preloadError", (event) => {
    const now = Date.now();
    if (!shouldReload(readLastReload(), now)) {
      // Second failure inside the window: let it surface rather than loop.
      return;
    }
    recordReload(now);
    event.preventDefault();
    window.location.reload();
  });
};
