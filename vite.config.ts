import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
// @ts-ignore — vitest augments defineConfig when imported from vitest/config
/// <reference types="vitest" />

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
