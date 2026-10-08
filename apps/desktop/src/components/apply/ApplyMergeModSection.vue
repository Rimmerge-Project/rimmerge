<script setup lang="ts">
// The "Write merge mod" checkbox and what the merge mod would hold. It owns the `get_merge_mod`
// query and is mounted only while the Apply dialog's body is (PrimeVue renders a Dialog's content
// only while it is open). The query is gated by mounting, not by an `enabled` option: every
// observer of one Colada entry shares it, and `invalidateQueries` reads the *last* observer's
// options, so a per-dialog `enabled` would let a hidden dialog switch off the refresh of an open
// one (or of the Merge mod page).
import Checkbox from "primevue/checkbox";
import { computed, useId } from "vue";
import { useI18n } from "vue-i18n";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useMergeModQuery } from "@/queries/merge";
import { describeMergeModGroups } from "@/utils/format";

const { t, locale } = useI18n();
const { writeMergeMod } = useApplyDialogState();
const { data: mergeMod, status, isLoading } = useMergeModQuery();

/**
 * No usable answer yet. `isLoading` is included on purpose, against the house rule of
 * `isPending` for placeholders: after a session swap the entry keeps the old session's data with
 * `status: "success"` while it refetches, and that data may describe a replaced session. Only
 * this line and the checkbox wait on it; nothing unmounts, so the flicker the rule guards
 * against does not apply. A *failed* refetch is not unknown: it settles to no summary and an
 * enabled box, as a failed first fetch always did (there is no error UI for it).
 */
const isMergeModUnknown = computed(() => status.value === "pending" || isLoading.value);

/**
 * `null` while unknown, when the merge mod has nothing to write, and after a failed fetch. Data
 * is read only on `status === "success"`: Colada keeps the previous `data` on a failed refetch
 * (`status: "error"`), and that is the replaced session's answer.
 */
const groupsSummary = computed(() =>
  isMergeModUnknown.value || status.value !== "success"
    ? null
    : describeMergeModGroups(mergeMod.value?.entries ?? [], t, locale.value),
);

/** The one status line's text: loading, then the summary, then nothing. */
const statusText = computed(() =>
  isMergeModUnknown.value ? t("common.loading") : (groupsSummary.value ?? ""),
);

const statusId = useId();
const checkboxPt = computed(() => ({
  input: { "aria-describedby": isMergeModUnknown.value ? statusId : undefined },
}));
</script>

<template>
  <!-- One root, so the parent's gap does not add space for an empty status line. -->
  <div class="flex flex-col gap-1">
    <label class="flex items-center gap-2 text-sm">
      <Checkbox
        v-model="writeMergeMod"
        class="shrink-0"
        binary
        :disabled="isMergeModUnknown"
        :pt="checkboxPt"
        data-testid="apply-dialog-write-merge-mod-checkbox"
      />
      {{ t("apply.dialog.writeMergeModLabel") }}
    </label>
    <!-- One status element, always mounted, and only its text changes: a live region that is
       inserted already holding its text is not reliably announced, one that exists empty and then
       gets text is. Until the merge mod answers (it can take seconds on a large install) it reads
       "Loading…" and the checkbox cannot be toggled, so the gap does not read as "nothing to
       merge" and a write cannot be newly requested against an unknown merge mod. A tick made
       before a session swap stays ticked, though: the box is disabled, not reset. -->
    <p
      :id="statusId"
      role="status"
      class="text-text-faint pl-6 text-xs"
      data-testid="apply-dialog-merge-mod-status"
    >
      {{ statusText }}
    </p>
  </div>
</template>
