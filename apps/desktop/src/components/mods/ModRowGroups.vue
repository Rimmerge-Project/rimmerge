<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { formatList } from "@/i18n/format";
import type { GroupLabelDto } from "@/types/generated/GroupLabelDto";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS } from "@/utils/organization";

/**
 * The groups a mod is a direct member of. From the row's `@md` width they are outlined chips with
 * the group's name (text, never only a colour dot): at most two, then "+N" whose tooltip lists
 * every group. Below it the chips collapse to dots (at most four, then "+N"): a coloured group is
 * a filled dot in its colour, an uncoloured one an outlined dot, so colour is never the only cue.
 * The sentence naming every group stays as screen-reader text at every width (the chips and dots
 * are hidden from assistive technology so no group is read twice), and the tooltips, the info
 * panel and the "Groups..." dialog list them too. A chip or dot is a pointer-only button that
 * asks the page to scope the lists to that group; it stays out of the Tab order so the grid keeps
 * one tab stop per row (the groups panel is the keyboard route to the same scope).
 */
const { groups } = defineProps<{ groups: readonly GroupLabelDto[] }>();
const emit = defineEmits<{ scope: [groupId: number] }>();

const { t, locale } = useI18n();

const MAX_CHIPS = 2;
const MAX_DOTS = 4;
const shown = computed(() => groups.slice(0, MAX_CHIPS));
const hiddenCount = computed(() => Math.max(0, groups.length - MAX_CHIPS));
const shownDots = computed(() => groups.slice(0, MAX_DOTS));
const hiddenDotCount = computed(() => Math.max(0, groups.length - MAX_DOTS));

function chipTitle(group: GroupLabelDto): string {
  if (group.color === null) {
    return t("mods.groups.chipScope", { name: group.name });
  }
  return t("mods.groups.chipScopeWithColor", {
    name: group.name,
    color: t(COLOR_NAME_KEY[group.color]),
  });
}

const allNames = computed(() =>
  formatList(
    locale.value,
    groups.map((group) => group.name),
  ),
);
</script>

<template>
  <template v-if="groups.length > 0">
    <span
      class="sr-only"
      data-testid="mod-row-groups-text"
    >{{ t("mods.groups.allGroups", { groups: allNames }) }}</span>
    <span
      class="hidden min-w-0 shrink-0 items-center gap-1 overflow-hidden @md:flex"
      aria-hidden="true"
      data-testid="mod-row-groups"
    >
      <button
        v-for="group in shown"
        :key="group.id"
        type="button"
        tabindex="-1"
        class="border-border text-text-muted hover:bg-surface-2 inline-flex max-w-32 cursor-pointer items-center gap-1 rounded border px-1.5 py-0.5 text-xs"
        :title="chipTitle(group)"
        :data-testid="`mod-row-group-${group.id}`"
        @mousedown.prevent
        @click.stop="emit('scope', group.id)"
        @dblclick.stop
      >
        <span
          v-if="group.color !== null"
          class="size-2 shrink-0 rounded-full"
          :class="COLOR_SWATCH_CLASS[group.color]"
          aria-hidden="true"
        />
        <span class="truncate">{{ group.name }}</span>
      </button>
      <span
        v-if="hiddenCount > 0"
        class="text-text-muted shrink-0 text-xs"
        :title="t('mods.groups.allGroups', { groups: allNames })"
        data-testid="mod-row-groups-more"
      >+{{ hiddenCount }}</span>
    </span>
    <span
      class="flex min-w-0 shrink-0 items-center gap-1 @md:hidden"
      aria-hidden="true"
      data-testid="mod-row-dots"
    >
      <button
        v-for="group in shownDots"
        :key="group.id"
        type="button"
        tabindex="-1"
        class="flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-full"
        :title="chipTitle(group)"
        :data-testid="`mod-row-dot-${group.id}`"
        @mousedown.prevent
        @click.stop="emit('scope', group.id)"
        @dblclick.stop
      >
        <span
          class="size-2.5 rounded-full"
          :class="group.color === null ? 'border-text-muted border' : COLOR_SWATCH_CLASS[group.color]"
        />
      </button>
      <span
        v-if="hiddenDotCount > 0"
        class="text-text-muted shrink-0 text-xs"
        :title="t('mods.groups.allGroups', { groups: allNames })"
        data-testid="mod-row-dots-more"
      >+{{ hiddenDotCount }}</span>
    </span>
  </template>
</template>
