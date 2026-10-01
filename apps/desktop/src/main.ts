import { PiniaColada } from "@pinia/colada";
import { createPinia } from "pinia";
import PrimeVue, { usePrimeVue } from "primevue/config";
import ToastService from "primevue/toastservice";
import { useToast } from "primevue/usetoast";
import { createApp } from "vue";

import App from "@/App.vue";
import {
  createAppI18n,
  registerAppI18n,
  registerPrimeVueLocaleConfig,
  setAppLocale,
} from "@/i18n/i18n";
import { router } from "@/router";
import { setSessionLostHandler } from "@/services/ipc";
import { usePreferencesStore } from "@/stores/preferences";
import { useSessionStore } from "@/stores/session";
import { preset } from "@/theme";
import { setToastService, toastError, toastInfo } from "@/utils/toast";
import "@/e2e-mock-bootstrap";
import "primeicons/primeicons.css";
import "@/style.css";

const app = createApp(App);

app.use(createPinia());
app.use(router);

const i18n = createAppI18n();
app.use(i18n);
registerAppI18n(i18n);

// "system" (not a `.p-dark`-style class selector) follows the OS/browser
// preference directly — see decision 1: no in-app
// theme toggle.
app.use(PrimeVue, { theme: { preset, options: { darkModeSelector: "system" } } });
app.use(ToastService);

// `useToast()`/`usePrimeVue()` need an active injection context, which
// only exists inside a component's `setup()` or (as here)
// `app.runWithContext` — captured once so `utils/toast.ts`'s
// `toastError` and `i18n.ts`'s `setAppLocale` can use them later from
// contexts that have neither (the global `mutationOptions.onError`
// below, a bespoke `catch` block, or the language picker outside any
// PrimeVue-composable-aware call stack).
setToastService(app.runWithContext(() => useToast()));
// PrimeVue's own type marks `config.locale` optional (`PrimeVueLocaleOptions
// | undefined`), but `setup()` (`@primevue/core/config`) always
// populates it from its own built-in English defaults before this ever
// runs — this cast reflects that runtime guarantee, not a missing check.
registerPrimeVueLocaleConfig(
  app.runWithContext(() => usePrimeVue()).config.locale as unknown as Record<string, unknown>,
);

// Loaded and applied before `app.mount()` (below), from whatever locale
// `stores/preferences.ts` last persisted (`"system"` by default) — this
// avoids a flash of English before a zh-CN/pt-BR reader's own choice
// applies. Top-level `await` is fine in this ESM entrypoint.
await setAppLocale(usePreferencesStore().locale);

app.use(PiniaColada, {
  mutationOptions: {
    // Every mutation failure gets a toast — the one place that catches
    // whatever a component's own error handling doesn't already cover
    // (most mutations here have no bespoke error UI at all). A component
    // with its own inline error state (e.g. `ApplyDialog`) still shows
    // it too; this is a global backstop, not a replacement.
    onError: (error) => toastError(error),
  },
});

// Registered once here (not in `services/ipc.ts` itself, which stays
// free of both Pinia and Vue Router imports): any command failing with
// `session_lost`/`no_project_loaded` means the loaded project is gone,
// so every page built on it resets to the setup flow instead of limping
// along showing stale data or a confusing per-page error. `useSessionStore()`
// is called lazily, inside the handler, not at registration time — by
// the time a real IPC failure can happen the app is fully mounted, so an
// active Pinia instance is guaranteed.
//
// **`session_lost` specifically is transparently recoverable**:
// `apps/desktop/src-tauri/src/state.rs`'s `with_session`
// only ever returns this code after catching a panic (or, defensively, a
// lock poisoned some other way) and discarding its own copy of the
// `Session` — the paths it was loaded from never changed. Rather than
// re-running `LoadProject` invisibly inside that same failed command
// (considered and rejected — a silent multi-second rescan hidden inside
// an unrelated command's own failure is its own bad surprise, and
// `Session`'s own mutating methods have no clone-and-swap discipline to
// make resuming the *same* in-memory session safe regardless — see
// `with_session`'s own doc comment), `markLostForReload` keeps the known
// paths and this handler sends the user straight back through
// `SetupPage`'s own load flow — the same visible progress bar a manual
// reload shows, not a hidden one — via the `autoReload` query flag
// `SetupPage.vue` checks on mount. Falls back to an ordinary full
// `reset()` (silently, no toast) when there's nothing to reload from
// (`no_project_loaded`, or a `session_lost` with no project ever
// loaded — defensive; `with_session` can't actually produce that
// combination today).
setSessionLostHandler((code) => {
  const session = useSessionStore();
  if (code === "session_lost") {
    const paths = session.markLostForReload();
    if (paths !== null) {
      toastInfo(
        "Reloading your project",
        "An internal error interrupted your session. Reloading from where you left off…",
      );
      void router.replace({ name: "setup", query: { autoReload: "1" } });
      return;
    }
  }
  session.reset();
  void router.replace({ name: "setup" });
});

app.mount("#app");
