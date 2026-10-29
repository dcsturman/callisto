// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The store first: the card's module graph reaches it through a cycle that
// only resolves in this order, as it does in the app.
import "state/store";
import { ReloadChoices, ReloadMount, SpareGunners } from "components/controls/WeaponUse";
import { Weapon } from "lib/weapon";

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

const rack = { kind: "Missile", mount: { Turret: 3 } } as unknown as Weapon;
const mount = (weapon_id: number, missiles: number): ReloadMount => ({
  weapon_id,
  weapon: rack,
  capacity: { missiles: 12, torpedoes: 0, sand: 0 },
  loaded: { missiles, torpedoes: 0, sand: 0 },
});
const stores = { missiles: 30, torpedoes: 0, sand: 0 };

const rows = () => Array.from(container.querySelectorAll(".reload-row")).map((row) => row.textContent ?? "");

describe("the Reload choice", () => {
  it("heads the card with the spare gunners left", () => {
    act(() => root.render(<SpareGunners spares={3} reloads={[2]} />));
    expect(container.textContent).toBe("Spare gunners: 2 of 3 free");
  });

  it("offers a reload by spare crew while one is free, and says what is loaded and in stores", () => {
    const onReload = vi.fn();
    act(() =>
      root.render(
        <ReloadChoices
          mounts={[mount(2, 6), mount(3, 12)]}
          stores={stores}
          spares={1}
          reloads={[]}
          ordered={[2, 3]}
          onReload={onReload}
          onWithdraw={vi.fn()}
        />,
      ),
    );
    const [low, full] = rows();
    expect(low).toContain("Mount 3");
    expect(low).toContain("Reload (6 of 12 loaded; 30 in stores)");
    expect(low).toContain("reloaded by spare crew");
    // A full rack is shown, but there is nothing to reload.
    expect(full).toContain("12 of 12 missiles");
    expect(container.querySelectorAll("button")).toHaveLength(1);

    act(() => (container.querySelector("button") as HTMLButtonElement).click());
    expect(onReload).toHaveBeenCalledWith(2);
  });

  it("with no spare free, a reload is instead of firing, and a chosen one can be withdrawn", () => {
    const onWithdraw = vi.fn();
    act(() =>
      root.render(
        <ReloadChoices
          mounts={[mount(2, 0), mount(3, 3)]}
          stores={stores}
          spares={1}
          reloads={[3, 2]}
          ordered={[2]}
          onReload={vi.fn()}
          onWithdraw={onWithdraw}
        />,
      ),
    );
    const [own, spare] = rows();
    expect(own).toContain("reloads instead of firing; its order waits a round");
    expect(spare).toContain("reloaded by spare crew");
    act(() => (container.querySelectorAll("button")[0] as HTMLButtonElement).click());
    expect(onWithdraw).toHaveBeenCalledWith(2);
  });

  it("says when stores are empty", () => {
    act(() =>
      root.render(
        <ReloadChoices
          mounts={[mount(2, 0)]}
          stores={{ missiles: 0, torpedoes: 0, sand: 0 }}
          spares={0}
          reloads={[]}
          ordered={[]}
          onReload={vi.fn()}
          onWithdraw={vi.fn()}
        />,
      ),
    );
    expect(rows()[0]).toContain("0 of 12 missiles; nothing in stores");
  });
});
