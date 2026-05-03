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
