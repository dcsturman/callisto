import { useEffect, useState } from "react";
import {
  googleLogout,
  useGoogleLogin,
  CodeResponse,
} from "@react-oauth/google";
import { login } from "./ServerManager";

export function Authentication(args: {
  setAuthenticated: (authenticated: boolean) => void;
  setEmail: (email: string | null) => void;
}) {
  const [googleAuthResponse, setGoogleAuthResponse] = useState<any>(null);
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
    onError: (error: any) => console.log("Login Failed:", error),
    flow: "auth-code",
    state: secureState,
    redirect_uri: process.env.REACT_APP_C_BACKEND || "http://localhost:50001",
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
    function loginToCallisto(code: string) {
      console.log("Logging in to Callisto");
      login(googleAuthResponse.code, args.setEmail, args.setAuthenticated);
    }

    console.log(
      "(Authentication) Redirect URI (REACT_APP_C_BACKEND) is set to: " +
        process.env.REACT_APP_C_BACKEND
    );
    if (googleAuthResponse) {
      if (googleAuthResponse.state !== secureState) {
        console.error("(Authentication) State mismatch, ignoring response");
        alert(
          "Authentication issue: not getting back pre-provided state.  Serious bug or MitM attack?"
        );
        return;
      }

      loginToCallisto(googleAuthResponse.code);
    }
  }, [args, googleAuthResponse, secureState]);

  return (
    <div className="authentication-container">
      <h1 className="authentication-title">
        Callisto{!process.env.REACT_APP_TUTORIAL ? " Tutorial" : ""}
      </h1>
      <br />
      <br />
      <div className="authentication-blurb">
        Welcome to Callisto! Callisto is a space combat simulator based loosely
        the Traveler universe. With Callisto you can deploy ships, steer around
        planets, and battle each other in medium sized space engagements. All
        movement is based on real physics and the built in flight computer
        attempt to help humans pilot in this complex environment.
        <br />
        <br />
        Callisto is currently in <em>closed alpha</em>. If you have been
        pre-authorized to trial Callisto please log in with your Google Id.
      </div>

      <br />
      {!process.env.REACT_APP_RUN_TUTORIAL ? (
        <>
          <br />
          <br />
          <button
            className="blue-button"
            onClick={() =>
              window.location.replace(`https://tutorial.${window.location.host}`)
            }>
            Go to Tutorial
          </button>
        </>
      ) : (
        <div className="authentication-blurb">
          Welcome, and sign in to run the tutorial! When you finish the tutorial
          you will be redirected back to the main server.
          <br /> <br />
          <br />
        </div>
      )}
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
          let token = btoa(stateToken.toString());
          setSecureState(token);
        }}>
        Sign in with Google{" "}
      </button>
    </div>
  );
}

export function Logout(args: {
  setAuthenticated: (authenticated: boolean) => void;
  email: string | null;
  setEmail: (email: string | null) => void;
}) {
  const logOut = () => {
    googleLogout();

    args.setAuthenticated(false);
    args.setEmail(null);
  };

  let username = args.email ? args.email.split("@")[0] : "";
  return (
    <div className="logout-window">
      <button className="blue-button" onClick={logOut}>
        Logout {username}
      </button>
    </div>
  );
}

export default Authentication;
