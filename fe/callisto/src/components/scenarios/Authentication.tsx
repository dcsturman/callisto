import { useEffect, useRef, useState } from "react";
import * as React from "react";
import {
  googleLogout,
  useGoogleLogin,
  CodeResponse,
} from "@react-oauth/google";
import { login, logout, register } from "lib/serverManager";
import { setEmail } from "state/userSlice";
import {
  setAuthenticated,
  setAuthBanner,
  authBannerSelector,
} from "state/serverSlice";

import { useAppDispatch, useAppSelector } from "state/hooks";

type AuthMode = "login" | "register";

export function Authentication() {
  const [googleAuthResponse, setGoogleAuthResponse] = useState<CodeResponse | null>(null);
  const [secureState, setSecureState] = useState<string | undefined>();
  // Tracks the mode at the moment we triggered the OAuth popup. We use a ref
  // (rather than `useState`) so the OAuth callback effect closes over the
  // most recent value without needing to re-create the effect or re-trigger
  // the popup when the mode toggles.
  const pendingModeRef = useRef<AuthMode>("login");

  const dispatch = useAppDispatch();
  const authBanner = useAppSelector(authBannerSelector);

  /**
 * The out of the box version of useGoogleLogin is missing options on the type signature.  So to make this wor
 * I had to "Go to Definition" and modify to look like this:
 interface AuthCodeFlowOptions extends Omit<CodeClientConfig, 'client_id' | 'scope' | 'callback'> {
    onSuccess?: (codeResponse: Omit<CodeResponse, 'error' | 'error_description' | 'error_uri'>) => void;
    onError?: (errorResponse: Pick<CodeResponse, 'error' | 'error_description' | 'error_uri'>) => void;
    onNonOAuthError?: (nonOAuthError: NonOAuthError) => void;
    scope?: CodeResponse['scope'];
    overrideScope?: boolean;
    accessType?: 'offline' | 'online';
    isSignedIn: boolean;
    responseType?: 'code' | 'token';
    prompt?: '' | 'none' | 'consent' | 'select_account';
  }
 */
  const googleLogin = useGoogleLogin({
    onSuccess: (codeResponse: CodeResponse) =>
      setGoogleAuthResponse(codeResponse),
    onError: (errorResponse: Pick<CodeResponse, 'error' | 'error_description' | 'error_uri'>) => console.log("Login Failed:", errorResponse),
    flow: "auth-code",
    state: secureState,
    // Redirect_uri should be the address of the Node.js server.
    redirect_uri: import.meta.env.VITE_NODE_SERVER || window.location.href,
    accessType: "offline",
    isSignedIn: true,
    responseType: "code",
    prompt: "consent",
    ux_mode: "popup",
  });

  useEffect(() => {
    // I don't like doing this is a effect hook.
    if (secureState) googleLogin();
  }, [googleLogin, secureState]);

  useEffect(() => {
    function authToCallisto() {
      if (!googleAuthResponse) {
        console.error("No code received from Google");
        return;
      }
      if (pendingModeRef.current === "register") {
        console.log("Registering with Callisto");
        register(googleAuthResponse.code);
      } else {
        console.log("Logging in to Callisto");
        login(googleAuthResponse.code);
      }
    }

    if (googleAuthResponse) {
      if (googleAuthResponse.state !== secureState) {
        console.error("(Authentication) State mismatch, ignoring response");
        return;
      }

      authToCallisto();
    }
  }, [googleAuthResponse, secureState]);

  function triggerOAuth() {
    // initialize SubtleCrypto
    const operations = window.crypto.subtle;

    // if Web Crypto is not supported, notify the user
    if (!operations) {
      alert("Web Crypto is not supported on this browser");
      console.warn("Web Crypto API not supported");
    }
    const stateToken = window.crypto.getRandomValues(new Uint8Array(48));
    const token = btoa(stateToken.toString());
    setSecureState(token);
  }

  function onSignIn() {
    dispatch(setAuthBanner(null));
    pendingModeRef.current = "login";
    triggerOAuth();
  }

  function onRegister() {
    dispatch(setAuthBanner(null));
    pendingModeRef.current = "register";
    triggerOAuth();
  }

  // Build a banner from any pinned auth error.
  let bannerNode: React.ReactNode = null;
  if (authBanner === "ALREADY_REGISTERED") {
    bannerNode = (
      <div role="alert" className="auth-banner auth-banner-info">
        You are already registered for Callisto &mdash; please use Sign in with Google to continue.
      </div>
    );
  } else if (authBanner === "NOT_AUTHORIZED") {
    bannerNode = (
      <div role="alert" className="auth-banner auth-banner-error" style={{ color: "red" }}>
        This Google account is not permitted to use Callisto.
      </div>
    );
  } else if (authBanner === "REGISTRATION_FAILED") {
    bannerNode = (
      <div role="alert" className="auth-banner auth-banner-error" style={{ color: "red" }}>
        Registration could not complete &mdash; please try again in a moment.
      </div>
    );
  } else if (authBanner === "AUTH_FAILED") {
    bannerNode = (
      <div role="alert" className="auth-banner auth-banner-error" style={{ color: "red" }}>
        Sign-in failed &mdash; please try again.
      </div>
    );
  }

  return (
    <div className="authentication-container">
      <h1 className="authentication-title">Callisto 1.0</h1>
      <br />
      <br />
      <div className="authentication-blurb">
        Welcome to Callisto! Callisto is a space combat simulator based
        the Traveler universe. Callisto is a <em>vector-based</em> ship combat system.  A vector-based
        system is based on the physics rules governing object motion, so the only tool for piloting a craft
        is via acceleration - which then changes your velocity and thus position.  There is no banking or rapid breaking
        in a vector-based system!
        <p />
        With Callisto you can deploy ships, steer around
        planets, and battle each other in medium sized space engagements. All
        movement is based on real physics and the built in flight computer
        attempt to help humans pilot in this complex environment.
        <a href="https://github.com/dcsturman/callisto/blob/main/callisto/FAQ.md">
        This FAQ </a>
        provides more details on how the game mechanics
        differ from the traditional Mongoose Traveller ship combat system.
        <br />
        <br />
        Callisto is open to anyone with a Google account &mdash; sign in to play, or click Register if this is your first visit.
      </div>

      <br />
      {bannerNode}
      <button className="blue-button" onClick={onSignIn}>
        Sign in with Google{" "}
      </button>
      <button className="blue-button" onClick={onRegister}>
        Register{" "}
      </button>
    </div>
  );
}

export function Logout() {
  const email = useAppSelector(state => state.user.email);
  const dispatch = useAppDispatch();

  const logOut = () => {
    googleLogout();
    dispatch(setAuthenticated(false));
    dispatch(setEmail(null));

    logout();
    console.log("(Authentication.Logout)Logged out");
  };

  const username = email ? email.split("@")[0] : "";
  return (
    <div className="logout-window">
      <button className="blue-button logout-button" onClick={logOut}>
        Logout {username}
      </button>
    </div>
  );
}

export default Authentication;
