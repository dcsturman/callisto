import * as React from "react";
import ReactDOM from "react-dom/client";
import * as Sentry from "@sentry/react";
import { Provider } from "react-redux";
import { store, persistor } from "./state/store";
import { GoogleOAuthProvider } from "@react-oauth/google";
import "./index.css";
import { GOOGLE_OAUTH_CLIENT_ID, App } from "./App";
import { PersistGate } from "redux-persist/integration/react";

const sentryDsn = import.meta.env.VITE_SENTRY_DSN;
if (sentryDsn) {
  Sentry.init({
    dsn: sentryDsn,
    environment: import.meta.env.VITE_SENTRY_ENVIRONMENT ?? import.meta.env.MODE,
    integrations: [
      Sentry.browserTracingIntegration(),
      Sentry.replayIntegration(),
    ],
    tracesSampleRate: 0.1,
    replaysSessionSampleRate: 0,
    replaysOnErrorSampleRate: 1.0,
  });
}

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement
);

document.body.style.overflow = "hidden";

console.groupCollapsed("Callisto Config parameters");
if (import.meta.env.VITE_CALLISTO_BACKEND) {
  console.log(
    "VITE_CALLISTO_BACKEND is set to: " + import.meta.env.VITE_CALLISTO_BACKEND
  );
} else {
  console.log("VITE_CALLISTO_BACKEND is not set.");
  console.log("ENV is set to: " + JSON.stringify(import.meta.env));
}

console.log("Running on " + window.location.href);
console.groupEnd();

root.render(
  <Sentry.ErrorBoundary fallback={<div>Something went wrong.</div>}>
    <GoogleOAuthProvider clientId={GOOGLE_OAUTH_CLIENT_ID}>
      <React.StrictMode>
        <Provider store={store}>
          <PersistGate loading={null} persistor={persistor}>
            <App />
          </PersistGate>
        </Provider>
      </React.StrictMode>
    </GoogleOAuthProvider>
  </Sentry.ErrorBoundary>
);
