import { useEffect, useState } from "react";
import {
  googleLogout,
  useGoogleLogin,
  CodeResponse,
} from "@react-oauth/google";
import { login } from "./ServerManager";

export function Authentication(args: {
  setAuthToken: (token: string | null ) => void;
  setEmail: (email: string | null ) => void;
}) {
  const [googleAuthResponse, setGoogleAuthResponse] = useState<any>(null);
  const [secureState, setSecureState] = useState<string | undefined>();

  const googleLogin = useGoogleLogin({
    onSuccess: (codeResponse: CodeResponse) => setGoogleAuthResponse(codeResponse),
    onError: (error) => console.log("Login Failed:", error),
    flow: 'auth-code',
    state: secureState,
    redirect_uri: "http://localhost:50001",
    accessType: "offline",
    isSignedIn: true,
    responseType: "code",
    prompt: "consent",
    ux_mode: "popup",
  });

  function loginToCallisto(
    code: string
  ) {
    console.log("Logging in to Callisto");
    login(googleAuthResponse.code, args.setEmail, args.setAuthToken);
  }

  useEffect(() => {
    // I don't like doing this is a effect hook.
    if (secureState) googleLogin();
  }, [googleLogin, secureState]);

  useEffect(() => {
    if (googleAuthResponse) {
      if (googleAuthResponse.state !== secureState) {
        console.error(
          "(Authentication) State mismatch, ignoring response"
        );
        alert(
          "Authentication issue: not getting back pre-provided state.  Serious bug or MitM attack?"
        );
        return;
      }

      loginToCallisto(googleAuthResponse.code);
    }
  }, [args, googleAuthResponse]);

  return (
    <div className="authentication-container">
      <h1 className="authentication-title">Callisto</h1>
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
      <br />
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
          let token = btoa(stateToken.toString());
          setSecureState(token);
        }}>
        Sign in with Google{" "}
      </button>
    </div>
  );
}

export function Logout(args: {
  setAuthToken: (token: string | null) => void;
  email: string | null;
  setEmail: (email: string | null) => void;
}) {
  const logOut = () => {
    googleLogout();

    args.setAuthToken(null);
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
