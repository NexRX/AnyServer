/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import solidPlugin from "vite-plugin-solid";

// For E2E tests, allow dynamic proxy target via environment variable
const proxyTarget =
  process.env.VITE_API_PROXY_TARGET || "http://localhost:3001";

export default defineConfig({
  plugins: [solidPlugin()],
  server: {
    port: 3000,
    proxy: {
      "/api": {
        target: proxyTarget,
        changeOrigin: true,
        ws: true,
      },
    },
  },
  preview: {
    port: 3000,
    proxy: {
      "/api": {
        target: proxyTarget,
        changeOrigin: true,
        ws: true,
      },
    },
  },
  build: {
    target: "esnext",
  },
  test: {
    environment: "jsdom",
    globals: true,
    exclude: ["e2e/**", "node_modules/**"],
    typecheck: {
      tsconfig: "./tsconfig.test.json",
    },
  },
});
