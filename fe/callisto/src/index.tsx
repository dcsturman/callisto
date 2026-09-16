import * as React from "react";
import ReactDOM from "react-dom/client";
import * as Sentry from "@sentry/react";
import { Provider } from "react-redux";
import { store, persistor } from "./state/store";
import { GoogleOAuthProvider } from "@react-oauth/google";
import "./index.css";
import { GOOGLE_OAUTH_CLIENT_ID, App } from "./App";
import { PersistGate } from "redux-persist/integration/react";
import { installStaleChunkReload } from "./lib/staleChunk";


// Announce which build this is, so a stale bundle is something you can check
// rather than infer from whether a change seems to have taken effect.
// Readable three ways: the console on load, `window.callistoBuild` at any
// time, and a data-build attribute on <html> that shows up in devtools and in
// a plain `curl` of the page.
const BUILD = { id: __BUILD_ID__, built: __BUILD_TIME__ };
(window as unknown as { callistoBuild: typeof BUILD }).callistoBuild = BUILD;
document.documentElement.setAttribute("data-build", BUILD.id);
console.log(`Callisto build ${BUILD.id} (${BUILD.built})`);

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

// A deploy invalidates the chunk names this tab was built with; the first
// lazy import after one would otherwise land in the error boundary. Installed
// before anything can import.
installStaleChunkReload();

/**
 * What the boundary shows when something still gets through. The common
 * cause is a stale tab after a deploy, so the honest advice -- and a button
 * that gives it -- is to reload.
 */
function SomethingWentWrong() {
  return (
    <div className="fatal-error">
      <p>Something went wrong.</p>
      <p className="fatal-error-detail">
        If Callisto was just updated, this tab is out of date.
      </p>
      <button type="button" onClick={() => window.location.reload()}>
        Reload
      </button>
    </div>
  );
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
  <Sentry.ErrorBoundary fallback={<SomethingWentWrong />}>
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
