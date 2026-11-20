<script setup lang="ts">
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ModSelection } from "@/composables/useModSelection";

/** The part of a list's selection the bar reads and acts on; pruning stays with the pane. */
type SelectionBarModel = Pick<
  ModSelection,
  | "shownCount"
  | "selectedCount"
  | "isCountPaused"
  | "areAllShownSelected"
  | "areSomeShownSelected"
  | "hiddenSelectedCount"
  | "selectAllShown"
  | "deselectShown"
  | "clear"
>;

/**
 * A Mods list's selection header: select all shown (or deselect them), how many are selected and
 * how many of those the filter hides (a polite live region), and clear.
 */
const { selection } = defineProps<{
  /** The list's selection, owned by its pane. */
  selection: SelectionBarModel;
}>();

const { t } = useI18n();

const hasRows = computed(() => selection.shownCount.value > 0);
const selectAllLabel = computed(() =>
  hasRows.value
    ? t(
        "mods.selection.selectAllShown",
        { count: selection.shownCount.value },
        selection.shownCount.value,
      )
    : t("mods.selection.selectAllEmpty"),
);

/**
 * One whole sentence, so the note about hidden rows (and any space or bracket around it) belongs
 * to the translation. Each number is worded by its own plural rule: "{selected}" and "{hidden}"
 * are two parts, each agreeing with its own count (ru, pl and uk change the noun with the number).
 */
const selectedText = computed(() => {
  const selected = t(
    "mods.selection.selectedCount",
    { count: selection.selectedCount.value },
    selection.selectedCount.value,
  );
  const hiddenCount = selection.hiddenSelectedCount.value;
  return hiddenCount > 0
    ? t("mods.selection.selectedCountHidden", {
        selected,
        hidden: t("mods.selection.hiddenPart", { count: hiddenCount }, hiddenCount),
      })
    : selected;
});

function toggleSelectAll(): void {
  if (selection.areAllShownSelected.value) {
    selection.deselectShown();
  } else {
    selection.selectAllShown();
  }
}
</script>

<template>
  <div
    class="flex shrink-0 flex-wrap items-center gap-3 text-sm"
    data-testid="mods-selection-bar"
  >
    <label class="flex items-center gap-1.5">
      <Checkbox
        binary
        :model-value="selection.areAllShownSelected.value"
        :indeterminate="selection.areSomeShownSelected.value"
        :disabled="!hasRows"
        data-testid="mods-select-all"
        @update:model-value="toggleSelectAll"
      />
      <span data-testid="mods-select-all-label">{{ selectAllLabel }}</span>
    </label>
    <span
      class="text-text-muted"
      role="status"
      aria-live="polite"
      data-testid="mods-selected-count"
    >
      <template v-if="selection.selectedCount.value > 0 && !selection.isCountPaused.value">
        {{ selectedText }}
      </template>
    </span>
    <Button
      :label="t('mods.selection.clear')"
      severity="secondary"
      text
      size="small"
      :disabled="selection.selectedCount.value === 0"
      data-testid="mods-clear-selection"
      @click="selection.clear"
    />
  </div>
</template>
