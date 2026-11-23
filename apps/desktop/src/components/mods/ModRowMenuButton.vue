<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

/**
 * A mod row ⋮ button: it opens the row actions menu (move or "Activate at…", and the mod
 * groups and colour), or, when `isDirect` (the menu would hold the one entry "Activate at…"), it
 * is that action itself and says so. It stays out of the Tab order (rows are one tab stop each;
 * Shift+F10 or the ContextMenu key is the keyboard route), and neither its click nor its keys
 * reach the row.
 */
const {
  modId,
  name,
  isDirect = false,
} = defineProps<{
  modId: string;
  /** The mod as the list labels it, for the button name and tooltip. */
  name: string;
  /** Whether the button runs "Activate at…" directly instead of opening a menu. */
  isDirect?: boolean;
}>();
const emit = defineEmits<{ open: [event: MouseEvent] }>();

const { t } = useI18n();

const label = computed(() =>
  isDirect ? t("mods.move.activateAtButton", { name }) : t("mods.table.rowActionsButton", { name }),
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
    :aria-haspopup="isDirect ? 'dialog' : 'menu'"
    :aria-label="label"
    :title="label"
    :data-testid="`mod-row-menu-${modId}`"
    @click.stop="(event: MouseEvent) => emit('open', event)"
    @dblclick.stop
    @keydown.stop
  />
</template>
