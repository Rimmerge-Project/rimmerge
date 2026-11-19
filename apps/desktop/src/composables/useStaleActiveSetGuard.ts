import { computed, ref, watch } from "vue";

import { usePendingActiveChangesQuery } from "@/queries/activeSet";
import { hasOwnPendingChanges } from "@/utils/pendingActiveChanges";

/**
 * Module-scoped, not component-scoped: [`isStaleOrUnapplied`] and
 * [`requestStaleConfirmation`] are read/called from `router.ts`'s own
 * navigation guard, which runs outside any component's setup and so
 * can't call [`usePendingActiveChangesQuery`] itself — the one live
 * [`useStaleActiveSetGuard`] instance ([`PendingChangesCloseGuard.vue`],
 * mounted once in `TheShell.vue`) keeps this state current instead, the
 * same "a module holds the shared state, a callback bridges into it"
 * shape `services/ipc.ts`'s `setSessionLostHandler` already uses.
 */
const isStale = ref(false);
const confirmVisible = ref(false);
let resolveConfirm: ((leave: boolean) => void) | null = null;

/**
 * Whether the working active-mod set currently has pending changes a
 * rescan hasn't picked up, or a scanned order (a different membership or only
 * a different sequence, as after a move) hasn't yet been applied to
 * `ModsConfig.xml` — the "warn on close/switch" check. Only the user's own work counts: an
 * outside edit of the file alone never trips it (the backend measures against what Rimmerge last
 * loaded or wrote). Safe to call from outside a component.
 */
export function isStaleOrUnapplied(): boolean {
  return isStale.value;
}

/**
 * Opens the shared confirm dialog ([`PendingChangesCloseGuard.vue`]) and
 * resolves once the user picks "Stay" (`false`) or "Leave anyway"
 * (`true`). Both the window-close handler and `router.ts`'s own
 * project-switch guard call this — only one confirmation is ever pending
 * at a time in practice (a user can't close the window and switch
 * projects in the same instant), so a single shared `resolveConfirm`
 * slot is enough.
 */
export function requestStaleConfirmation(): Promise<boolean> {
  confirmVisible.value = true;
  return new Promise((resolve) => {
    resolveConfirm = resolve;
  });
}

/** Resolves whatever confirmation {@link requestStaleConfirmation} opened. */
function resolveConfirmation(leave: boolean): void {
  confirmVisible.value = false;
  resolveConfirm?.(leave);
  resolveConfirm = null;
}

/**
 * Keeps the module-scoped {@link isStaleOrUnapplied} reading current from
 * a live [`usePendingActiveChangesQuery`], and exposes the confirm
 * dialog's own visible/resolve pair for [`PendingChangesCloseGuard.vue`]
 * to render.
 */
export function useStaleActiveSetGuard() {
  const { data: pending } = usePendingActiveChangesQuery();
  const staleOrUnapplied = computed(() => {
    const changes = pending.value;
    return changes !== undefined && hasOwnPendingChanges(changes);
  });
  watch(staleOrUnapplied, (value) => (isStale.value = value), { immediate: true });

  return { confirmVisible, resolveConfirmation };
}
