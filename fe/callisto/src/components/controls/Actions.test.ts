import { describe, expect, it } from "vitest";
import { ActionType, actionPayload, payloadToAction } from "components/controls/Actions";

const slot = (extra: Partial<ActionType[string]>): ActionType[string] => ({
  sensors: [],
  fire: [],
  unfire: [],
  pointDefense: [],
  engineers: [],
  leadershipCheck: null,
  clearSensors: [],
  clearEngineers: [],
  clearLeadership: false,
  ...extra,
});

describe("flight orders on the wire", () => {
  it("sends the carrier's Launch, and an empty one to withdraw it", () => {
    const [[, queued]] = actionPayload({ Sirocco: slot({ launch: ["Gale 01", "Gale 02"] }) });
    expect(queued).toEqual([{ Launch: { craft: ["Gale 01", "Gale 02"] } }]);
    const [[, withdrawn]] = actionPayload({ Sirocco: slot({ launch: null, clearLaunch: true }) });
    expect(withdrawn).toEqual([{ Launch: { craft: [] } }]);
    const [[, nothing]] = actionPayload({ Sirocco: slot({}) });
    expect(nothing).toEqual([]);
  });

  it("sends the craft's Land, and ClearLand to withdraw it", () => {
    const [[, queued]] = actionPayload({ "Gale 01": slot({ land: "Sirocco" }) });
    expect(queued).toEqual([{ Land: { carrier: "Sirocco" } }]);
    const [[, withdrawn]] = actionPayload({ "Gale 01": slot({ land: null, clearLand: true }) });
    expect(withdrawn).toEqual(["ClearLand"]);
  });

  it("reads them back from the server's queue", () => {
    const parsed = payloadToAction([
      ["Sirocco", [{ Launch: { craft: ["Gale 01"] } }]],
      ["Gale 02", [{ Land: { carrier: "Sirocco" } }]],
      ["Gale 03", []],
    ]);
    expect(parsed.Sirocco.launch).toEqual(["Gale 01"]);
    expect(parsed.Sirocco.land).toBeNull();
    expect(parsed["Gale 02"].land).toBe("Sirocco");
    expect(parsed["Gale 02"].launch).toBeNull();
    expect(parsed["Gale 03"].launch).toBeNull();
    expect(parsed["Gale 03"].clearLand).toBe(false);
  });
});
