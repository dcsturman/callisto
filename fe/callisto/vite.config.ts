import { sentryVitePlugin } from "@sentry/vite-plugin";
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';
import { execSync } from 'child_process';
import svgr from 'vite-plugin-svgr';

// Source-map upload is opt-in: only runs when SENTRY_AUTH_TOKEN is set (CI),
// so plain `npm run build` locally still works without Sentry credentials.
const sentryAuthToken = process.env.SENTRY_AUTH_TOKEN;

// A stamp identifying exactly which commit produced this bundle, so "am I
// looking at a stale version?" is answerable rather than inferred from whether
// a change appears to have taken effect.  GitHub Actions sets GITHUB_SHA; a
// local build falls back to git, and to "dev" outside a checkout.
function buildId(): string {
  const sha =
    process.env.GITHUB_SHA ??
    (() => {
      try {
        return execSync("git rev-parse HEAD", { stdio: ["ignore", "pipe", "ignore"] })
          .toString()
          .trim();
      } catch {
        return "";
      }
    })();
  return sha ? sha.slice(0, 8) : "dev";
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
    svgr({
      // svgr options: https://react-svgr.com/docs/options/
      svgrOptions: {
        exportType: 'default',
        ref: true,
        svgo: false,
        titleProp: true,
      },
      include: '**/*.svg?react',
    }),
    ...(sentryAuthToken
      ? [sentryVitePlugin({
          org: "self-vt0",
          project: "callisto-fe",
          authToken: sentryAuthToken,
        })]
      : []),
  ],
  resolve: {
    alias: {
      // Map the non-relative imports to src directory
      'components': path.resolve(__dirname, './src/components'),
      'lib': path.resolve(__dirname, './src/lib'),
      'state': path.resolve(__dirname, './src/state'),
      'assets': path.resolve(__dirname, './src/assets'),
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 3000,
    open: true,
  },
  build: {
    outDir: 'build',
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          // Split vendor code into separate chunks for better caching
          'react-vendor': ['react', 'react-dom'],
          'three-vendor': ['three'],
          'three-react': ['@react-three/fiber'],
          'three-helpers': ['@react-three/drei', '@react-three/postprocessing'],
          'animation': ['@react-spring/three', '@react-spring/core'],
          'redux': ['@reduxjs/toolkit', 'react-redux', 'redux-persist'],
        },
      },
    },
  },
  define: {
    // Replace process.env with import.meta.env for Vite
    'process.env': {},
    __BUILD_ID__: JSON.stringify(buildId()),
    __BUILD_TIME__: JSON.stringify(new Date().toISOString()),
  },
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'three',
      '@react-three/fiber',
      '@react-three/drei',
      '@react-three/postprocessing',
      '@react-spring/three',
    ],
  },
});
