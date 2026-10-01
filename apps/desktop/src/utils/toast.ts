import type { ToastServiceMethods } from "primevue/toastservice";

import { translateMessage } from "@/i18n/i18n";
import { describeCommandError } from "@/utils/errors";

let toast: ToastServiceMethods | null = null;

/**
 * Registers the app's toast service instance. Called once from
 * `main.ts` via `app.runWithContext(() => useToast())` — `useToast()`
 * itself needs an active component/app injection context, which the
 * global Pinia Colada `mutationOptions.onError` this module backs never
 * has (it runs from the mutation's own internal execution, not from
 * inside a component's `setup()`).
 */
export function setToastService(service: ToastServiceMethods): void {
  toast = service;
}

/**
 * Shows an error toast for `error`, localized by its `CommandErrorCode`
 * (see {@link describeCommandError}) — `main.ts`'s global Pinia Colada
 * `mutationOptions.onError` is this function's one caller today, so
 * every command failure not already handled by a component's own error
 * UI gets this identical, code-driven summary/detail pair. A no-op
 * before {@link setToastService} has run (never the case once the app
 * is mounted) or in a Vitest unit test that never registers one.
 */
export function toastError(error: unknown): void {
  const described = describeCommandError(error);
  toast?.add({
    severity: "error",
    summary: translateMessage(described.title),
    detail: translateMessage(described.detail),
    life: 6000,
  });
}

/**
 * Shows a generic informational toast — currently only `main.ts`'s
 * global session-lost handler, to explain a sudden, otherwise-unexplained
 * jump back to the setup page (a caught backend panic triggering an
 * automatic reload) rather than leaving the user to wonder what
 * happened.
 */
export function toastInfo(summary: string, detail?: string): void {
  toast?.add({
    severity: "info",
    summary,
    ...(detail !== undefined ? { detail } : {}),
    life: 6000,
  });
}
