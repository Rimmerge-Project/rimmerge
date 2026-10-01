<script setup lang="ts">
// The row editor's "Copy from" chips: one per existing match, the winner badged.
import { useI18n } from "vue-i18n";
import { useRowEditorContext } from "@/composables/rowEditorContext";

const { descriptor, modLabel, copyFromMatch, isCopying } = useRowEditorContext();
const { t } = useI18n();
</script>

<template>
  <div
    v-if="descriptor.matches.length > 0"
    class="flex flex-wrap gap-1"
    data-testid="row-editor-copy-from"
  >
    <span class="text-text-muted text-xs">{{ t("assignments.rowEditor.copyFrom") }}</span>
    <button
      v-for="match in descriptor.matches"
      :key="match.instanceDefName"
      type="button"
      class="bg-surface-2 hover:bg-surface-1 cursor-pointer rounded-full px-2 py-0.5 text-xs disabled:cursor-not-allowed"
      :disabled="isCopying"
      :data-testid="`row-editor-copy-from-${match.instanceDefName}`"
      @click="copyFromMatch(match.instanceDefName, match.owner)"
    >
      {{ match.instanceDefName }} ({{ modLabel.label(match.owner) }})
      <span
        v-if="descriptor.winner?.kind === 'existing' && descriptor.winner.match.instanceDefName === match.instanceDefName"
        class="text-status-input font-semibold"
        data-testid="row-editor-copy-from-winner-badge"
      >
        {{ t("assignments.rowEditor.winnerBadge") }}
      </span>
    </button>
  </div>
</template>
