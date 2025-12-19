/// <reference types="vite/client" />
/// <reference types="vite-plugin-svgr/client" />

interface ImportMetaEnv {
  readonly VITE_CALLISTO_BACKEND?: string;
  readonly VITE_NODE_SERVER?: string;
  readonly VITE_GOOGLE_OAUTH_CLIENT_ID?: string;
  // Add other env variables here as needed
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

