// @vitest-environment jsdom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { CornerPanel } from "lib/CornerPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("CornerPanel", () => {
  let root: Root | null = null;
  let container: HTMLDivElement | null = null;
  // The test browser has no storage of its own; a stand-in for this test.
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => store.set(key, value),
    });
  });
  afterEach(() => {
    act(() => root?.unmount());
    container?.remove();
    vi.unstubAllGlobals();
  });

  const mount = () => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() =>
      root!.render(
        <CornerPanel id="test-panel" title="Fleet Comms">
          <p className="body">reports</p>
        </CornerPanel>,
      ),
    );
  };

  it("folds its contents away and keeps its header, so it can be opened again", () => {
    mount();
    const toggle = () => container!.querySelector<HTMLButtonElement>(".corner-panel-toggle")!;
    expect(container!.querySelector(".body")).not.toBeNull();
    act(() => toggle().click());
    expect(container!.querySelector(".body")).toBeNull();
    expect(toggle().textContent).toContain("Fleet Comms");
    act(() => toggle().click());
    expect(container!.querySelector(".body")).not.toBeNull();
  });

  it("remembers being folded", () => {
    mount();
    act(() => container!.querySelector<HTMLButtonElement>(".corner-panel-toggle")!.click());
    act(() => root!.unmount());
    container!.remove();
    mount();
    expect(container!.querySelector(".body")).toBeNull();
  });
});
