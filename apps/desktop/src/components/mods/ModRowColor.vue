<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS } from "@/utils/organization";

/**
 * A mod's own colour on its row, in two parts the row places separately. The `stripe` is a 4 px
 * mark on the row's leading edge, hidden from assistive technology. The `name` is the colour's
 * word as screen-reader text and goes inside the row's name cell, so it is read with the mod
 * (text directly in a `row` would belong to no cell). The stripe is never the only cue: the name
 * is in the row text, the info panel and the row menu. Renders nothing for an uncoloured mod.
 */
const { color, part } = defineProps<{
  color: ColorTokenDto | null;
  part: "stripe" | "name";
}>();

const { t } = useI18n();
const name = computed(() => (color === null ? null : t(COLOR_NAME_KEY[color])));
</script>

<template>
  <template v-if="color !== null">
    <span
      v-if="part === 'stripe'"
      class="pointer-events-none absolute inset-y-0 left-0 w-1"
      :class="COLOR_SWATCH_CLASS[color]"
      :data-color="color"
      aria-hidden="true"
      data-testid="mod-row-color"
    />
    <span
      v-else
      class="sr-only"
      data-testid="mod-row-color-name"
    >{{ t("mods.colors.rowColor", { name }) }}</span>
  </template>
</template>
