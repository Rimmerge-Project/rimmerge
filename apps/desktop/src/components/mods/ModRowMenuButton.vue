<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { RowMenu } from "@/components/mods/ModTable.vue";

/**
 * A mod row's ⋮ button: the Move menu on the Active list, "Activate at…" on the Inactive list.
 * It stays out of the Tab order (rows are one tab stop each; Shift+F10 or the ContextMenu key is
 * the keyboard route), and neither its click nor its keys reach the row.
 */
const { modId, menu, name } = defineProps<{
  modId: string;
  /** Which action the button opens. */
  menu: Exclude<RowMenu, "none">;
  /** The mod as the list labels it, for the button's name and tooltip. */
  name: string;
}>();
const emit = defineEmits<{ open: [event: MouseEvent] }>();

const { t } = useI18n();

const label = computed(() =>
  menu === "move" ? t("mods.move.menuButton", { name }) : t("mods.move.activateAtButton", { name }),
);
</script>

<template>
  <Button
    class="shrink-0"
    icon="pi pi-ellipsis-v"
    severity="secondary"
    text
    rounded
    size="small"
    tabindex="-1"
    :aria-haspopup="menu === 'move' ? 'menu' : 'dialog'"
    :aria-label="label"
    :title="label"
    :data-testid="`mod-row-menu-${modId}`"
    @click.stop="(event: MouseEvent) => emit('open', event)"
    @keydown.stop
  />
</template>
