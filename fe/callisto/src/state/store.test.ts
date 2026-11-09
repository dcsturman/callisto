// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { MAX_PERSISTED_EVENTS, capEvents, store } from "state/store";
import { Event } from "components/space/Effects";

const ui = () => store.getState().ui;
const events = (n: number) => Array.from({ length: n }, (_, id) => ({ id, kind: "Message" }) as unknown as Event);

describe("what the UI slice keeps across a reload", () => {
  it("keeps a round's results of ordinary size", () => {
    const state = { ...ui(), events: events(10) };
    expect(capEvents.in(state, "ui", {}).events).toHaveLength(10);
  });

  it("drops a big battle's, rather than write them out on every frame", () => {
    const state = { ...ui(), events: events(MAX_PERSISTED_EVENTS + 1), gravityWells: true };
    const kept = capEvents.in(state, "ui", {});
    expect(kept.events).toBeNull();
    expect(kept.gravityWells).toBe(true);
  });
});
