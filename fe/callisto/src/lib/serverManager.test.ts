// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Stub WebSocket *before* importing serverManager so that startWebsocket()
// installs its onmessage handler on our mock instance instead of a real ws.
class MockSocket {
  sent: string[] = [];
  readyState = 1; // OPEN
  onopen: ((ev: Event) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    /* noop */
  }
}

let mockSocket: MockSocket;

beforeEach(async () => {
  // Reset module registry so the module-level `socket` re-binds to our mock.
  vi.resetModules();
  mockSocket = new MockSocket();
  const ctor = vi.fn().mockImplementation(() => mockSocket);
  // Mirror the static enum members the real WebSocket exposes — production
  // code reads `WebSocket.OPEN` and `WebSocket.CLOSED`.
  Object.assign(ctor, {
    CONNECTING: 0,
    OPEN: 1,
    CLOSING: 2,
    CLOSED: 3,
  });
  vi.stubGlobal("WebSocket", ctor);

  // Importing serverManager evaluates its top-level `socket` declaration
  // (undefined until startWebsocket is called).
  const sm = await import("lib/serverManager");
  sm.startWebsocket();
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("register", () => {
  it("sends a Register payload with the supplied OAuth code", async () => {
    const sm = await import("lib/serverManager");
    sm.register("foo");
    expect(mockSocket.sent).toEqual([
      JSON.stringify({ Register: { code: "foo" } }),
    ]);
  });
});

describe("Error inbound handling", () => {
  async function fireMessage(payload: unknown) {
    expect(mockSocket.onmessage).toBeTruthy();
    mockSocket.onmessage!(
      new MessageEvent("message", { data: JSON.stringify(payload) }),
    );
  }

  it.each([
    "NOT_AUTHORIZED",
    "ALREADY_REGISTERED",
    "REGISTRATION_FAILED",
    "AUTH_FAILED",
  ] as const)(
    "routes pinned auth Error %s through setAuthBanner",
    async (code) => {
      const { store } = await import("state/store");
      await fireMessage({ Error: code });
      expect(store.getState().server.authBanner).toBe(code);
    },
  );

  it("ignores non-pinned Error strings (no banner state change)", async () => {
    const { store } = await import("state/store");
    const initialBanner = store.getState().server.authBanner;
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
    await fireMessage({ Error: "SCENARIO_EXISTS" });
    expect(store.getState().server.authBanner).toBe(initialBanner);
    expect(alertSpy).toHaveBeenCalledWith("SCENARIO_EXISTS");
  });
});

describe("ScenarioLoadErrors inbound handling", () => {
  async function fireMessage(payload: unknown) {
    expect(mockSocket.onmessage).toBeTruthy();
    mockSocket.onmessage!(
      new MessageEvent("message", { data: JSON.stringify(payload) }),
    );
  }

  it("stores the failure list in serverSlice", async () => {
    const { store } = await import("state/store");
    await fireMessage({
      ScenarioLoadErrors: [
        { filename: "Marduk Encounter.json", error: "missing design" },
        { filename: "Other.json", error: "bad json" },
      ],
    });
    const errors = store.getState().server.scenarioLoadErrors;
    expect(errors).toHaveLength(2);
    expect(errors[0].filename).toBe("Marduk Encounter.json");
    expect(errors[1].error).toBe("bad json");
  });
});

describe("setShipEmissions", () => {
  it("sends a request to go dark", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipEmissions("Harrier", false);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({
        SetShipEmissions: { ship_name: "Harrier", active_sensors: false },
      }),
    ]);
  });

  it("sends a request to bring sensors back up", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipEmissions("Harrier", true);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({
        SetShipEmissions: { ship_name: "Harrier", active_sensors: true },
      }),
    ]);
  });

  it("can go silent without restating the sensor setting", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipEmissions("Harrier", undefined, false);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({
        SetShipEmissions: { ship_name: "Harrier", transmitting: false },
      }),
    ]);
  });

  it("can set both at once", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipEmissions("Harrier", false, false);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({
        SetShipEmissions: {
          ship_name: "Harrier",
          active_sensors: false,
          transmitting: false,
        },
      }),
    ]);
  });
});

describe("setShipTeam", () => {
  it("assigns a team", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipTeam("Harrier", "Red");
    expect(mockSocket.sent).toEqual([
      JSON.stringify({ SetShipTeam: { ship_name: "Harrier", team: "Red" } }),
    ]);
  });

  it("omits the team to make a ship unaligned", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipTeam("Harrier", null);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({ SetShipTeam: { ship_name: "Harrier" } }),
    ]);
  });
});

describe("sensor hand-off", () => {
  it("turns hand-off on without restating the other emissions", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipEmissions("Picket", undefined, undefined, true);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({
        SetShipEmissions: { ship_name: "Picket", handoff_sensors: true },
      }),
    ]);
  });

  it("can turn it off again", async () => {
    const sm = await import("lib/serverManager");
    sm.setShipEmissions("Picket", undefined, undefined, false);
    expect(mockSocket.sent).toEqual([
      JSON.stringify({
        SetShipEmissions: { ship_name: "Picket", handoff_sensors: false },
      }),
    ]);
  });
});
