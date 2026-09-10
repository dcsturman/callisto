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

describe("scenario dirty tracking", () => {
  async function dirty() {
    const { store } = await import("state/store");
    return store.getState().ui.scenarioDirty;
  }

  it("starts clean", async () => {
    expect(await dirty()).toBe(false);
  });

  it.each([
    ["removeEntity", (sm: typeof import("lib/serverManager")) => sm.removeEntity("Flayer")],
    [
      "renameEntity",
      (sm: typeof import("lib/serverManager")) => sm.renameEntity("Flayer", "Thrasher"),
    ],
  ])("marks the scenario dirty after %s", async (_name, mutate) => {
    const sm = await import("lib/serverManager");
    mutate(sm);
    expect(await dirty()).toBe(true);
  });

  it("clears the flag when the server confirms a save", async () => {
    const sm = await import("lib/serverManager");
    sm.removeEntity("Flayer");
    expect(await dirty()).toBe(true);

    mockSocket.onmessage!(
      new MessageEvent("message", {
        data: JSON.stringify({ ScenarioSaved: "mine.json" }),
      }),
    );
    expect(await dirty()).toBe(false);
  });

  it("clears the flag on leaving the scenario", async () => {
    const sm = await import("lib/serverManager");
    sm.removeEntity("Flayer");
    expect(await dirty()).toBe(true);

    sm.exit_scenario();
    expect(await dirty()).toBe(false);
  });

  it("clears the flag when a different scenario is joined", async () => {
    const sm = await import("lib/serverManager");
    sm.removeEntity("Flayer");
    expect(await dirty()).toBe(true);

    sm.joinScenario("sol.json");
    expect(await dirty()).toBe(false);
  });
});
