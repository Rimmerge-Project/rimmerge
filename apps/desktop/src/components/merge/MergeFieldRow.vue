<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import MergeValueCell from "@/components/merge/MergeValueCell.vue";
import { useModLabel } from "@/composables/useModLabel";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeOwnerDto } from "@/types/generated/MergeOwnerDto";

const {
  field,
  owners,
  active = false,
  activeColumn = -1,
  editing = false,
  editingDraft = "",
  isPatchCollision = false,
  pendingChoice = null,
} = defineProps<{
  field: MergeFieldDto;
  /** Every owner, in the selected order — one radio per entry (column 0 is the base). */
  owners: MergeOwnerDto[];
  /** Whether the row cursor is on this row. */
  active?: boolean;
  /** The column cursor's index into `owners`, meaningful only when `active`. */
  activeColumn?: number;
  /** Whether this row is mid-edit (an `e` press on it, not yet saved/canceled). */
  editing?: boolean;
  /** The edit's starting text, set once when `editing` turns on. */
  editingDraft?: string;
  /**
   * Whether this preview is for a `PatchCollision` finding, where a
   * `Drop` choice can never actually apply (RimWorld's own patches
   * replay to a value — there's no "drop" operation a collision's
   * replay can express). Disables the Drop button and the `d` shortcut
   * for this row.
   */
  isPatchCollision?: boolean;
  /**
   * A choice picked locally but not yet confirmed by the server (from
   * `useMergeChoices`' own client-side map) — takes priority over
   * `field.choice` everywhere this row renders "the current choice", so
   * a pick shows immediately instead of waiting out the debounce and
   * round trip, and a second pick made before the first settles still
   * reads the fresh local state rather than a stale server snapshot.
   * `null` means "nothing
   * pending" — fall back to `field.choice`.
   */
  pendingChoice?: MergeChoiceDto | null;
}>();
const emit = defineEmits<{
  select: [];
  choose: [choice: MergeChoiceDto];
  drop: [];
  revert: [];
  editCommit: [text: string];
  editCancel: [];
}>();

const CLASS_BADGE: Record<MergeFieldDto["class"], string> = {
  unchanged: "bg-surface-2 text-text-faint",
  oneSided: "bg-status-auto-soft text-status-auto",
  agreeing: "bg-status-auto-soft text-status-auto",
  conflict: "bg-status-input-soft text-status-input",
};

/** {@link MergeFieldDto.class}'s own badge text — kept as its own map (not a `describe*` helper) since it's read only here. */
const CLASS_LABEL_KEY: Record<MergeFieldDto["class"], string> = {
  unchanged: "merge.field.classUnchanged",
  oneSided: "merge.field.classOneSided",
  agreeing: "merge.field.classAgreeing",
  conflict: "merge.field.classConflict",
};

const { t } = useI18n();
const modLabel = useModLabel();
const draft = ref(editingDraft);
const inputRef = useTemplateRef<HTMLInputElement>("editInput");
watch(
  () => editing,
  async (isEditing) => {
    if (!isEditing) {
      return;
    }
    draft.value = editingDraft;
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  },
);

/** The choice this row currently shows as "current" — a pending local pick, or the server's own stored one. */
const effectiveChoice = computed(() => pendingChoice ?? field.choice);
/** A pending pick not yet confirmed by the server — the row's own "saving…" cue. */
const isPending = computed(
  () => pendingChoice !== null && JSON.stringify(pendingChoice) !== JSON.stringify(field.choice),
);

type ResultDisplay = { kind: "value"; text: string } | { kind: "dropped" } | { kind: "needsInput" };

/**
 * What the Result cell shows: an explicit `Value`/`From` choice renders
 * instantly from the client's own state (never waiting on the server to
 * confirm it, matching {@link effectiveChoice}); `Drop` and "no choice at
 * all" render distinctly from each other and from a genuine value,
 * instead of every one of the three collapsing into the same bare
 * "absent" span.
 */
const resultDisplay = computed<ResultDisplay>(() => {
  const choice = effectiveChoice.value;
  if (choice?.choice === "drop") {
    return { kind: "dropped" };
  }
  if (choice?.choice === "value") {
    return { kind: "value", text: choice.text };
  }
  const text = choice?.choice === "from" ? (field.candidates[choice.modId] ?? null) : field.result;
  return text === null ? { kind: "needsInput" } : { kind: "value", text };
});
</script>

<template>
  <div
    class="border-border-subtle flex items-center gap-2 border-b px-2 py-1"
    :class="active ? 'bg-accent-soft' : ''"
    :style="{ paddingLeft: `${field.depth * 12 + 8}px` }"
    data-testid="merge-field-row"
    @click="emit('select')"
  >
    <span
      class="w-40 shrink-0 truncate font-mono text-xs"
      :title="field.path"
    >{{ field.path }}</span>
    <span
      class="w-20 shrink-0 rounded-full px-1.5 py-0.5 text-center text-[10px] font-medium whitespace-nowrap"
      :class="CLASS_BADGE[field.class]"
    >{{ t(CLASS_LABEL_KEY[field.class]) }}</span>
    <div
      role="radiogroup"
      :aria-label="t('merge.field.chooseValueFor', { path: field.path })"
      class="contents"
    >
      <label
        v-for="(owner, index) in owners"
        :key="owner.modId"
        class="flex w-32 shrink-0 items-center gap-1 rounded px-1"
        :class="[
          active && activeColumn === index ? 'ring-accent ring-2' : '',
          !effectiveChoice && field.preselected === owner.modId ? 'bg-surface-2' : '',
        ]"
      >
        <input
          type="radio"
          :name="`merge-choice-${field.path}`"
          :checked="effectiveChoice?.choice === 'from' && effectiveChoice.modId === owner.modId"
          :data-testid="`merge-radio-${field.path}-${owner.modId}`"
          :aria-label="t('merge.field.takeValue', { owner: modLabel.label(owner.modId) })"
          class="focus-visible:outline"
          @change="emit('choose', { choice: 'from', modId: owner.modId })"
        >
        <MergeValueCell
          :value="field.candidates[owner.modId] ?? null"
          :base="field.base"
          :is-xml="field.entry !== 'leaf'"
        />
        <span
          v-if="!effectiveChoice && field.preselected === owner.modId"
          class="text-text-faint text-[9px] italic"
          data-testid="merge-preselected-hint"
        >{{ t("merge.field.suggestedHint") }}</span>
      </label>
    </div>
    <div class="w-32 shrink-0">
      <input
        v-if="editing"
        ref="editInput"
        v-model="draft"
        type="text"
        class="border-border-strong bg-surface-0 text-text w-full rounded border px-1 py-0.5 text-xs"
        data-testid="merge-edit-input"
        @keydown.enter="emit('editCommit', draft)"
        @keydown.esc="emit('editCancel')"
        @click.stop
      >
      <template v-else>
        <span
          v-if="resultDisplay.kind === 'dropped'"
          class="text-status-danger text-xs italic"
          data-testid="merge-result-dropped"
        >{{ t("merge.field.dropped") }}</span>
        <span
          v-else-if="resultDisplay.kind === 'needsInput'"
          class="text-status-input text-xs italic"
          data-testid="merge-result-needs-input"
        >{{ t("merge.field.needsInput") }}</span>
        <MergeValueCell
          v-else
          :value="resultDisplay.text"
          :base="field.base"
          :is-xml="field.entry !== 'leaf'"
        />
        <span
          v-if="isPending"
          class="text-text-faint text-[10px] italic"
          data-testid="merge-result-pending"
        >{{ t("merge.field.saving") }}</span>
      </template>
    </div>
    <div class="flex shrink-0 items-center gap-1">
      <button
        v-if="!isPatchCollision"
        type="button"
        class="cursor-pointer rounded border px-1.5 py-0.5 text-[11px] focus-visible:outline"
        :class="
          effectiveChoice?.choice === 'drop'
            ? 'border-status-danger bg-status-danger-soft text-status-danger'
            : 'border-border-subtle text-text-muted'
        "
        data-testid="merge-drop-button"
        @click.stop="emit('drop')"
      >
        {{ t("merge.field.drop") }}
      </button>
      <button
        v-else
        type="button"
        disabled
        class="border-border-subtle text-text-faint cursor-not-allowed rounded border px-1.5 py-0.5 text-[11px] opacity-60"
        data-testid="merge-drop-unavailable"
        :title="t('merge.field.dropUnavailableTitle')"
      >
        {{ t("merge.field.drop") }}
      </button>
      <button
        v-if="effectiveChoice"
        type="button"
        class="border-border-subtle text-text-muted cursor-pointer rounded border px-1.5 py-0.5 text-[11px] focus-visible:outline"
        data-testid="merge-revert-button"
        @click.stop="emit('revert')"
      >
        {{ field.class === "conflict" ? t("merge.field.clearChoice") : t("merge.field.autoRevert") }}
      </button>
    </div>
  </div>
</template>
