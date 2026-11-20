<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { formatList } from "@/i18n/format";
import type { GroupLabelDto } from "@/types/generated/GroupLabelDto";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS } from "@/utils/organization";

/**
 * The groups a mod is a direct member of, as outlined chips with the group's name (text, never
 * only a colour dot). At most two chips show, then "+N" whose tooltip lists every group. Below
 * the row's `@md` width the chips are hidden, like tags; the sentence naming every group stays as
 * screen-reader text at every width (the chips are hidden from assistive technology so no group
 * is read twice), and the info panel and the "Groups..." dialog list them too.
 */
const { groups } = defineProps<{ groups: readonly GroupLabelDto[] }>();

const { t, locale } = useI18n();

const MAX_CHIPS = 2;
const shown = computed(() => groups.slice(0, MAX_CHIPS));
const hiddenCount = computed(() => Math.max(0, groups.length - MAX_CHIPS));

function chipTitle(group: GroupLabelDto): string {
  return group.color === null
    ? group.name
    : t("mods.groups.chipWithColor", { name: group.name, color: t(COLOR_NAME_KEY[group.color]) });
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
      <span
        v-for="group in shown"
        :key="group.id"
        class="border-border text-text-muted inline-flex max-w-32 items-center gap-1 rounded border px-1.5 py-0.5 text-xs"
        :title="chipTitle(group)"
        :data-testid="`mod-row-group-${group.id}`"
      >
        <span
          v-if="group.color !== null"
          class="size-2 shrink-0 rounded-full"
          :class="COLOR_SWATCH_CLASS[group.color]"
          aria-hidden="true"
        />
        <span class="truncate">{{ group.name }}</span>
      </span>
      <span
        v-if="hiddenCount > 0"
        class="text-text-muted shrink-0 text-xs"
        :title="t('mods.groups.allGroups', { groups: allNames })"
        data-testid="mod-row-groups-more"
      >+{{ hiddenCount }}</span>
    </span>
  </template>
</template>
