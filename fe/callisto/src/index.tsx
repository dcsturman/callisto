import React from "react";
import ReactDOM from "react-dom/client";
import { GoogleOAuthProvider } from "@react-oauth/google";
import "./index.css";
import { GOOGLE_OAUTH_CLIENT_ID, App } from "./App";
import reportWebVitals from "./reportWebVitals";

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
if (process.env.REACT_APP_RUN_TUTORIAL) {
  console.log("Tutorial is set to run.");
} else {
  console.log("Tutorial is not set to run.");
}
console.groupEnd();

root.render(
  <GoogleOAuthProvider clientId={GOOGLE_OAUTH_CLIENT_ID}>
    <React.StrictMode>
      <App />
    </React.StrictMode>
  </GoogleOAuthProvider>
);

// If you want to start measuring performance in your app, pass a function
// to log results (for example: reportWebVitals(console.log))
// or send to an analytics endpoint. Learn more: https://bit.ly/CRA-vitals
reportWebVitals();
