// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The store first: the card's module graph reaches it through a cycle that
// only resolves in this order, as it does in the app.
import "state/store";
import { WeaponButton } from "components/controls/WeaponUse";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a spinal mount on the gunner card", () => {
  it("is one button with its multiple, and says what it cannot attack", () => {
    act(() =>
      root.render(
        <WeaponButton weapon="Meson" mount={{ Spinal: 3 }} count={1} onClick={() => {}} disabled={false} />,
      ),
    );
    const button = container.querySelector("#Meson-spinal-button");
    expect(button).not.toBeNull();
    expect(button!.textContent).toContain("3×");
    // Its own icon, a barrel the length of the hull, not a bay's.
    const icon = button!.querySelector("svg.spinal-button");
    expect(icon).not.toBeNull();
    expect(icon!.getAttribute("data-testid")).toBe("spinal-icon");
    expect(button!.querySelector("svg.bay-button")).toBeNull();
    const tip = button!.getAttribute("data-tooltip-content") ?? "";
    expect(tip).toContain("Meson Spinal Mount (3×)");
    expect(tip).toContain("2,000 tons");
  });
});
