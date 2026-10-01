<script setup lang="ts">
import Badge from "primevue/badge";
import Button from "primevue/button";
import type { PopoverMethods } from "primevue/popover";
import Popover from "primevue/popover";
import { computed, nextTick, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import NotificationList from "@/components/notifications/NotificationList.vue";
import { useNotificationsQuery } from "@/queries/notifications";

const { t } = useI18n();
const { data: notifications } = useNotificationsQuery();

const count = computed(() => notifications.value?.length ?? 0);
const bellLabel = computed(() =>
  count.value > 0
    ? t("notifications.bell.withCount", { count: count.value }, count.value)
    : t("notifications.bell.none"),
);

const popover = useTemplateRef<PopoverMethods>("popover");
const panel = useTemplateRef<HTMLDivElement>("panel");
const isOpen = ref(false);

function toggle(event: Event): void {
  popover.value?.toggle(event);
}

/**
 * Moves focus to the popover's first focusable element once it opens —
 * a dialog-like popover must not leave focus stranded on the trigger
 * button. `nextTick` waits for the panel's own `v-if`-gated content
 * (`NotificationList`) to have actually rendered before the query runs.
 */
async function onShow(): Promise<void> {
  isOpen.value = true;
  await nextTick();
  panel.value
    ?.querySelector<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    )
    ?.focus();
}

function onHide(): void {
  isOpen.value = false;
}
</script>

<template>
  <!--
    The badge is a sibling of the button, not a child of it: a PrimeVue
    `Button` clips its overflow to its rounded box, which cut off the top
    of a count hanging off its corner.
  -->
  <div class="relative">
    <Button
      text
      rounded
      aria-haspopup="dialog"
      :aria-expanded="isOpen"
      :aria-label="bellLabel"
      data-testid="notification-bell"
      @click="toggle"
    >
      <i
        class="pi pi-bell text-base"
        aria-hidden="true"
      />
    </Button>
    <Badge
      v-if="count > 0"
      :value="count > 99 ? '99+' : String(count)"
      severity="danger"
      class="pointer-events-none absolute top-0 right-0 origin-top-right translate-x-1.5 -translate-y-1/2"
      aria-hidden="true"
      data-testid="notification-bell-badge"
    />

    <Popover
      ref="popover"
      data-testid="notification-popover"
      @show="onShow"
      @hide="onHide"
    >
      <div
        ref="panel"
        role="dialog"
        aria-labelledby="notification-list-heading"
      >
        <NotificationList />
      </div>
    </Popover>
  </div>
</template>
