import {describe, expect, test} from "vitest";
import {RELOAD_WINDOW_MS, shouldReload} from "lib/staleChunk";

describe("shouldReload", () => {
  test("a tab that has never reloaded for this may do so", () => {
    expect(shouldReload(null, 1_000_000)).toBe(true);
  });

  test("a second failure inside the window is not retried", () => {
    // A chunk that is genuinely missing would otherwise reload forever.
    const now = 1_000_000;
    expect(shouldReload(now - 5_000, now)).toBe(false);
    expect(shouldReload(now - RELOAD_WINDOW_MS, now)).toBe(false);
  });

  test("after the window a fresh deploy may reload again", () => {
    const now = 1_000_000;
    expect(shouldReload(now - RELOAD_WINDOW_MS - 1, now)).toBe(true);
  });
});
