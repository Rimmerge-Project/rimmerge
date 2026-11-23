import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import basicSsl from "@vitejs/plugin-basic-ssl";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

// Tauri expects a fixed dev server port (matches `devUrl` in
// `src-tauri/tauri.conf.json`) and needs the dev server to keep running
// even when a command fails, plus to ignore `src-tauri/` (a Rust change
// there triggers a separate cargo rebuild, not a Vite reload).
// https://v2.tauri.app/start/frontend/vite/
//
// `RIMMERGE_E2E_HTTPS=1` (set only by the Playwright configs' `webServer`)
// serves the dev server over TLS with a throwaway self-signed certificate
// (made once and cached under `node_modules/.vite`, which is gitignored),
// which makes Vite speak HTTP/2. The e2e tier opens a fresh browser context
// per test; over HTTP/1.1 each opens about six keep-alive sockets that the
// browser closes first, so Windows piles up client-side TIME_WAIT entries and
// now and then hands out a port still in one, refusing a module request and
// leaving a blank page (#58). HTTP/2 needs about one connection per context.
// `tauri dev` never sets it: its fixed `devUrl` is plain http. Anything that
// turns `server.https` off (or a Vite that serves HTTPS over HTTP/1.1) brings
// #58 back; `specs/http2.spec.ts` catches it.
const isE2eHttps = process.env["RIMMERGE_E2E_HTTPS"] === "1";

export default defineConfig({
  plugins: [vue(), tailwindcss(), ...(isE2eHttps ? [basicSsl()] : [])],
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
