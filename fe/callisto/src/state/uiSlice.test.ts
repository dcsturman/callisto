import {describe, expect, test} from "vitest";

import uiReducer, {setEvents, removeEvent, clearMessageEvents} from "state/uiSlice";
import type {Event} from "components/space/Effects";

const event = (id: number, kind: string): Event => ({
  id,
  kind,
  content: null,
  position: [0, 0, 0],
  target: null,
  origin: null,
});

describe("event removal", () => {
  // Explosions finish in any order, each holding the list from when it
  // started. Removing by id against the store keeps one finishing from putting
  // back another that already finished.
  test("removing two events in turn leaves neither", () => {
    let state = uiReducer(undefined, setEvents([event(1, "ShipImpact"), event(2, "BeamHit"), event(3, "ShipImpact")]));
    state = uiReducer(state, removeEvent(1));
    state = uiReducer(state, removeEvent(3));
    expect(state.events?.map((e) => e.id)).toEqual([2]);
  });

  test("closing the results keeps explosions still running", () => {
    let state = uiReducer(undefined, setEvents([event(1, "Message"), event(2, "ShipImpact")]));
    state = uiReducer(state, clearMessageEvents());
    expect(state.events?.map((e) => e.id)).toEqual([2]);
  });
});
