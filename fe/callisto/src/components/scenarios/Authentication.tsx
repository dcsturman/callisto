import { useEffect, useState } from "react";
import * as React from "react";
import {
  googleLogout,
  useGoogleLogin,
  CodeResponse,
} from "@react-oauth/google";
import { login, logout } from "lib/serverManager";
import { setEmail } from "state/userSlice";
import { setAuthenticated } from "state/serverSlice";

import { useAppDispatch, useAppSelector } from "state/hooks";

export function Authentication() {
  const [googleAuthResponse, setGoogleAuthResponse] = useState<CodeResponse | null>(null);
  const [secureState, setSecureState] = useState<string | undefined>();

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
    redirect_uri: process.env.REACT_APP_NODE_SERVER || window.location.href,
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
    function loginToCallisto() {
      console.log("Logging in to Callisto");
      if (googleAuthResponse) {
        login(googleAuthResponse.code);
      } else {
        console.error("No code received from Google");
      }
    }

    // Uncomment when debugging but don't generally want this in the logs in the client.
    //console.log("(Authentication) OAuth ClientID = " + GOOGLE_OAUTH_CLIENT_ID);

    console.log(process.env);

    if (googleAuthResponse) {
      if (googleAuthResponse.state !== secureState) {
        console.error("(Authentication) State mismatch, ignoring response");
        return;
      }

      loginToCallisto();
    }
  }, [googleAuthResponse, secureState]);

  return (
    <div className="authentication-container">
      <h1 className="authentication-title">Callisto 0.2&beta;</h1>
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
        Callisto is currently in <em style={{"color" : "red"}}>closed beta</em>. If you have been
        pre-authorized to trial Callisto please log in with your Google Id.  Otherwise please be patient!  
        We will have a more broadly open beta soon followed by opening Callisto up to everyone!
      </div>

      <br />
      <button
        className="blue-button"
        onClick={() => {
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
        }}>
        Sign in with Google{" "}
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
