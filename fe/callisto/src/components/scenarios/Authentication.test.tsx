// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

// Tell React this is an `act()`-aware environment so it doesn't warn on
// every state update during tests.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// We avoid @testing-library/react here because @testing-library/dom is not
// installed as a dependency (peer-dep of testing-library/react that would
// require touching package.json — out of scope for this agent). Plain
// createRoot + document queries get the job done for these assertions.

// Capture the onSuccess handler that Authentication wires into useGoogleLogin
// so we can synthesize an OAuth callback in tests.
let capturedOnSuccess: ((codeResp: { code: string; state?: string }) => void) | null = null;
let capturedState: string | undefined = undefined;

vi.mock("@react-oauth/google", () => {
  return {
    googleLogout: vi.fn(),
    useGoogleLogin: (opts: {
      onSuccess: (resp: { code: string; state?: string }) => void;
      state?: string;
    }) => {
      capturedOnSuccess = opts.onSuccess;
      capturedState = opts.state;
      return () => {
        /* noop — popup not opened in tests */
      };
    },
    CodeResponse: {},
  };
});

const registerMock = vi.fn();
const loginMock = vi.fn();
const logoutMock = vi.fn();
vi.mock("lib/serverManager", () => ({
  register: (code: string) => registerMock(code),
  login: (code: string) => loginMock(code),
  logout: () => logoutMock(),
}));

import { Authentication } from "components/scenarios/Authentication";
import { store } from "state/store";
import { setAuthBanner } from "state/serverSlice";

let container: HTMLDivElement;
let root: Root;

function renderAuth() {
  act(() => {
    root.render(
      <Provider store={store}>
        <Authentication />
      </Provider>,
    );
  });
}

function getButton(label: RegExp): HTMLButtonElement {
  const btn = Array.from(container.querySelectorAll("button")).find((b) =>
    label.test(b.textContent ?? ""),
  );
  if (!btn) {
    throw new Error(`Button matching ${label} not found`);
  }
  return btn as HTMLButtonElement;
}

function click(btn: HTMLButtonElement) {
  act(() => {
    btn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
}

beforeEach(() => {
  registerMock.mockClear();
  loginMock.mockClear();
  capturedOnSuccess = null;
  capturedState = undefined;
  store.dispatch(setAuthBanner(null));
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  vi.clearAllMocks();
});

describe("Splash copy and version", () => {
  it("shows the 1.0 version string", () => {
    renderAuth();
    expect(container.textContent).toContain("Callisto 1.0");
  });

  it("shows the GA splash copy (no closed-beta wording)", () => {
    renderAuth();
    expect(container.textContent).toMatch(
      /open to anyone with a Google account/i,
    );
    expect(container.textContent).not.toMatch(/closed beta/i);
  });

  it("renders both Sign In and Register buttons", () => {
    renderAuth();
    expect(getButton(/sign in with google/i)).toBeTruthy();
    expect(getButton(/^Register/)).toBeTruthy();
  });
});

describe("Register flow", () => {
  it("clicking Register triggers OAuth and dispatches register(code) on completion", () => {
    renderAuth();
    click(getButton(/^Register/));

    expect(capturedOnSuccess).toBeTruthy();
    expect(capturedState).toBeDefined();

    act(() => {
      capturedOnSuccess!({ code: "abc123", state: capturedState });
    });

    expect(registerMock).toHaveBeenCalledWith("abc123");
    expect(loginMock).not.toHaveBeenCalled();
  });

  it("clicking Sign In triggers OAuth and dispatches login(code)", () => {
    renderAuth();
    click(getButton(/sign in with google/i));

    expect(capturedOnSuccess).toBeTruthy();
    act(() => {
      capturedOnSuccess!({ code: "logincode", state: capturedState });
    });

    expect(loginMock).toHaveBeenCalledWith("logincode");
    expect(registerMock).not.toHaveBeenCalled();
  });
});

describe("Banner rendering", () => {
  it("NOT_AUTHORIZED renders the not-permitted banner", () => {
    act(() => {
      store.dispatch(setAuthBanner("NOT_AUTHORIZED"));
    });
    renderAuth();
    expect(container.textContent).toMatch(/not permitted to use Callisto/i);
  });

  it("REGISTRATION_FAILED renders the try-again banner", () => {
    act(() => {
      store.dispatch(setAuthBanner("REGISTRATION_FAILED"));
    });
    renderAuth();
    expect(container.textContent).toMatch(/Registration could not complete/i);
  });

  it("AUTH_FAILED renders the sign-in failed banner", () => {
    act(() => {
      store.dispatch(setAuthBanner("AUTH_FAILED"));
    });
    renderAuth();
    expect(container.textContent).toMatch(/Sign-in failed/i);
  });

  it("ALREADY_REGISTERED shows the already-registered banner without auto-retriggering OAuth", () => {
    renderAuth();

    // Server replies with ALREADY_REGISTERED. The component should leave the
    // banner up and NOT re-trigger any OAuth flow on its own.
    act(() => {
      store.dispatch(setAuthBanner("ALREADY_REGISTERED"));
    });

    expect(container.textContent).toMatch(/already registered/i);
    expect(store.getState().server.authBanner).toBe("ALREADY_REGISTERED");
    expect(loginMock).not.toHaveBeenCalled();
    expect(registerMock).not.toHaveBeenCalled();

    // When the user clicks Sign In, the banner clears and OAuth runs fresh
    // in login mode.
    click(getButton(/^Sign in/));
    expect(store.getState().server.authBanner).toBeNull();
    expect(capturedOnSuccess).toBeTruthy();
    act(() => {
      capturedOnSuccess!({ code: "freshcode", state: capturedState });
    });
    expect(loginMock).toHaveBeenCalledWith("freshcode");
    expect(registerMock).not.toHaveBeenCalled();
  });
});
