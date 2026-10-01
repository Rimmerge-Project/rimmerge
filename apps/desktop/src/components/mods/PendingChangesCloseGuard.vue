<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { onMounted, onScopeDispose } from "vue";
import { useI18n } from "vue-i18n";

import {
  isStaleOrUnapplied,
  requestStaleConfirmation,
  useStaleActiveSetGuard,
} from "@/composables/useStaleActiveSetGuard";

const { t } = useI18n();
const { confirmVisible, resolveConfirmation } = useStaleActiveSetGuard();

/**
 * Warns before the window closes while the working active-mod set has
 * pending changes
 * — mirrors `useTauriEvent`'s own disposal-safety shape (an in-flight
 * `onCloseRequested` registration that resolves after this scope
 * disposes must unregister immediately, not leak a listener calling into
 * a gone component).
 */
let unlisten: (() => void) | undefined;
let disposed = false;

onMounted(() => {
  void getCurrentWindow()
    .onCloseRequested(async (event) => {
      if (!isStaleOrUnapplied()) {
        return;
      }
      event.preventDefault();
      const leave = await requestStaleConfirmation();
      if (leave) {
        try {
          await getCurrentWindow().destroy();
        } catch (error) {
          // A denied permission (see `core:window:allow-destroy` in
          // `capabilities/default.json`) or any other `destroy()` failure
          // must not fail silently — the window would just stay open with
          // no visible cause. The guard stays fail-open either way: it has
          // already called `preventDefault`, so a failed `destroy()` simply
          // leaves the window open, same as before this call.
          console.error("PendingChangesCloseGuard: window.destroy() failed", error);
        }
      }
    })
    .then((stop) => {
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
    })
    .catch((error: unknown) => {
      // Registering the listener itself can fail (e.g. a denied
      // permission on the listen path) — the guard stays fail-open (no
      // listener registered means Tauri closes the window natively), but
      // that failure should still be visible instead of silently dropped.
      console.error("PendingChangesCloseGuard: onCloseRequested registration failed", error);
    });
});

onScopeDispose(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <Dialog
    :visible="confirmVisible"
    modal
    :header="t('mods.closeGuard.header')"
    data-testid="stale-close-confirm-dialog"
    @update:visible="(value: boolean) => !value && resolveConfirmation(false)"
  >
    <!-- eslint-disable vue/no-bare-strings-in-template -- a literal
         filename inside `<code>`, never translated, the same category
         as a keyboard-shortcut label; every other string in this block
         already goes through `t()`. -->
    <i18n-t
      keypath="mods.closeGuard.body"
      tag="p"
      class="max-w-sm text-sm"
    >
      <template #modsConfig>
        <code>ModsConfig.xml</code>
      </template>
    </i18n-t>
    <!-- eslint-enable vue/no-bare-strings-in-template -->
    <template #footer>
      <Button
        :label="t('mods.closeGuard.stay')"
        severity="secondary"
        data-testid="stale-close-confirm-stay"
        @click="resolveConfirmation(false)"
      />
      <Button
        :label="t('mods.closeGuard.leaveAnyway')"
        severity="danger"
        data-testid="stale-close-confirm-leave"
        @click="resolveConfirmation(true)"
      />
    </template>
  </Dialog>
</template>
