import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

// Tauri expects a fixed dev server port (matches `devUrl` in
// `src-tauri/tauri.conf.json`) and needs the dev server to keep running
// even when a command fails, plus to ignore `src-tauri/` (a Rust change
// there triggers a separate cargo rebuild, not a Vite reload).
// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
    // Transforms the app's own entry chain right away instead of on the
    // first request — Playwright's mock-IPC tier throws several browser
    // contexts at a freshly started (CI) or just-edited (local) dev
    // server at once, and without this the first concurrent burst of
    // dynamic-import requests can hit mid-transform and 500.
    warmup: {
      clientFiles: ["./src/main.ts", "./src/App.vue", "./src/router.ts"],
    },
  },
  optimizeDeps: {
    // Pre-bundled up front for the same reason `server.warmup` is:
    // discovering one of these mid-request (the default, lazy behavior)
    // triggers a dependency re-optimization and a full page reload,
    // which is exactly the kind of disruption a concurrent test run
    // can't tolerate on a cold server.
    include: [
      "vue",
      "vue-router",
      "pinia",
      "@pinia/colada",
      "@tauri-apps/api/core",
      // `NotificationBell.vue` (mounted unconditionally in `TheShell.vue`,
      // same as every other PrimeVue component already on this list's
      // own reasoning) is the first user of either — added explicitly
      // after the mock-IPC tier flaked under 4 parallel workers on a
      // cold server until these two were pre-bundled up front.
      "primevue/popover",
      "primevue/badge",
    ],
  },
});
