import * as React from "react";
import ReactDOM from "react-dom/client";
import { Provider } from "react-redux";
import { store } from "./state/store";
import { GoogleOAuthProvider } from "@react-oauth/google";
import "./index.css";
import { GOOGLE_OAUTH_CLIENT_ID, App } from "./App";

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement
);

document.body.style.overflow = "hidden";

console.groupCollapsed("Callisto Config parameters");
if (process.env.REACT_APP_CALLISTO_BACKEND) {
  console.log(
    "REACT_APP_CALLISTO_BACKEND is set to: " + process.env.REACT_APP_CALLISTO_BACKEND
  );
} else {
  console.log("REACT_APP_CALLISTO_BACKEND is not set.");
  console.log("ENV is set to: " + JSON.stringify(process.env));
}

console.log("Running on " + window.location.href);
console.groupEnd();
console.log("(index.tsx) store = " + JSON.stringify(store));

root.render(
  <GoogleOAuthProvider clientId={GOOGLE_OAUTH_CLIENT_ID}>
    <React.StrictMode>
      <Provider store={store}>
        <App />
      </Provider>
    </React.StrictMode>
  </GoogleOAuthProvider>
);
