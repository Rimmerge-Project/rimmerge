<script setup lang="ts">
import { nextTick, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import type { PaneView } from "@/utils/panePrefs";

/**
 * One Mods pane's `List | Groups` switch (a `radiogroup`: one tab stop, arrows move and choose),
 * and, while the pane shows Groups, "Collapse all" and "Expand all". Groups needs a usable
 * organization; while it is not, the choice is disabled and says why. Display only: it changes how
 * the same mods are laid out, never which mods or what order they load in.
 */
const { isGroupsAvailable } = defineProps<{
  /** Whether the Groups view can be used (organization is loaded). */
  isGroupsAvailable: boolean;
}>();
const emit = defineEmits<{ collapseAll: []; expandAll: [] }>();
const view = defineModel<PaneView>("view", { required: true });

const { t } = useI18n();
const group = useTemplateRef<HTMLDivElement>("viewGroup");
const VIEWS: readonly PaneView[] = ["list", "groups"];

function isDisabled(choice: PaneView): boolean {
  return choice === "groups" && !isGroupsAvailable;
}

function choose(choice: PaneView): void {
  if (!isDisabled(choice)) {
    view.value = choice;
  }
}

async function onKeydown(event: KeyboardEvent): Promise<void> {
  const step =
    event.key === "ArrowRight" || event.key === "ArrowDown"
      ? 1
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? -1
        : 0;
  if (step === 0) {
    return;
  }
  event.preventDefault();
  const next = VIEWS[(VIEWS.indexOf(view.value) + step + VIEWS.length) % VIEWS.length];
  if (next === undefined || isDisabled(next)) {
    return;
  }
  view.value = next;
  await nextTick();
  group.value?.querySelector<HTMLElement>(`[data-view="${next}"]`)?.focus();
}
</script>

<template>
  <div class="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1 text-sm">
    <span class="text-text-muted">{{ t("mods.groupsView.switchHeading") }}</span>
    <div
      ref="viewGroup"
      role="radiogroup"
      class="flex"
      :aria-label="t('mods.groupsView.switchLabel')"
      data-testid="mods-view-switch"
      @keydown="onKeydown"
    >
      <button
        v-for="choice in VIEWS"
        :key="choice"
        type="button"
        role="radio"
        class="border-border hover:bg-surface-2 focus-visible:outline-accent aria-checked:bg-accent-soft aria-checked:text-accent aria-checked:border-accent inline-flex h-7 cursor-pointer items-center border px-3 first:rounded-l last:rounded-r aria-disabled:cursor-not-allowed aria-disabled:opacity-60 focus-visible:outline-2"
        :aria-checked="view === choice"
        :aria-disabled="isDisabled(choice) ? 'true' : undefined"
        :tabindex="view === choice ? 0 : -1"
        :title="isDisabled(choice) ? t('mods.groupsView.unavailable') : undefined"
        :data-view="choice"
        :data-testid="`mods-view-${choice}`"
        @click="choose(choice)"
      >
        {{ choice === "list" ? t("mods.groupsView.viewList") : t("mods.groupsView.viewGroups") }}
      </button>
    </div>
    <span
      v-if="!isGroupsAvailable"
      class="text-text-muted text-xs"
      data-testid="mods-view-unavailable"
    >{{ t("mods.groupsView.unavailable") }}</span>
    <template v-if="view === 'groups'">
      <button
        type="button"
        class="border-border hover:bg-surface-2 focus-visible:outline-accent inline-flex h-7 cursor-pointer items-center rounded border px-2 focus-visible:outline-2"
        data-testid="mods-groups-collapse-all"
        @click="emit('collapseAll')"
      >
        {{ t("mods.groupsView.collapseAll") }}
      </button>
      <button
        type="button"
        class="border-border hover:bg-surface-2 focus-visible:outline-accent inline-flex h-7 cursor-pointer items-center rounded border px-2 focus-visible:outline-2"
        data-testid="mods-groups-expand-all"
        @click="emit('expandAll')"
      >
        {{ t("mods.groupsView.expandAll") }}
      </button>
    </template>
  </div>
</template>
