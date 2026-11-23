<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { GroupLabelDto } from "@/types/generated/GroupLabelDto";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS, firstColoredGroup } from "@/utils/organization";

/**
 * A mod's colour on its row, in two parts the row places separately. The `stripe` is a 4 px mark
 * on the row's leading edge, hidden from assistive technology. The `name` is the colour's word as
 * screen-reader text and goes inside the row's name cell, so it is read with the mod (text
 * directly in a `row` would belong to no cell). The stripe is the mod's own colour; with none, it
 * is the colour of the mod's first coloured group (the backend sends the groups in tree order),
 * drawn faded so it reads as inherited, and the screen-reader text says it is the group's colour,
 * never the mod's. The stripe is never the only cue: the name is in the row text, the info panel
 * and the row menu. Renders nothing for a mod with neither.
 */
const {
  color,
  groups = [],
  part,
} = defineProps<{
  /** The mod's own colour. */
  color: ColorTokenDto | null;
  /** The mod's groups, in group-tree order; read only while the mod has no colour of its own. */
  groups?: readonly GroupLabelDto[];
  part: "stripe" | "name";
}>();

const { t } = useI18n();

const inherited = computed(() => (color === null ? firstColoredGroup(groups) : null));
const shownColor = computed<ColorTokenDto | null>(() => color ?? inherited.value?.color ?? null);
const text = computed(() => {
  const own = color;
  if (own !== null) {
    return t("mods.colors.rowColor", { name: t(COLOR_NAME_KEY[own]) });
  }
  const group = inherited.value;
  return group === null
    ? ""
    : t("mods.colors.rowColorFromGroup", {
        name: t(COLOR_NAME_KEY[group.color]),
        group: group.name,
      });
});
</script>

<template>
  <template v-if="shownColor !== null">
    <span
      v-if="part === 'stripe'"
      class="pointer-events-none absolute inset-y-0 left-0 w-1"
      :class="[COLOR_SWATCH_CLASS[shownColor], inherited === null ? '' : 'opacity-40']"
      :data-color="shownColor"
      :data-inherited="inherited === null ? undefined : 'true'"
      aria-hidden="true"
      data-testid="mod-row-color"
    />
    <span
      v-else
      class="sr-only"
      data-testid="mod-row-color-name"
    >{{ text }}</span>
  </template>
</template>
