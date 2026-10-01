<script setup lang="ts">
// The apply dialog's read-only preflight: unapplied active changes and the profile paths.
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink } from "vue-router";
import ApplyProblemList from "@/components/apply/ApplyProblemList.vue";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { orderSourceLabel } from "@/utils/orderSource";

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const {
  session,
  preflight,
  close,
  modLabel,
  unappliedActiveChanges,
  hasUnappliedActiveChanges,
  modsConfigPath,
  decisionsPath,
  rulesPath,
  movedMods,
  sortProvenance,
  needsInput,
  orderFixableOperations,
  partlyFixableOperations,
  pairKeyOf,
  createdPairRules,
  existingUserPairKeys,
} = useApplyDialogState();

/** The unapplied-active-changes lists, joined for display through the locale's own list conjunction rather than a hard-coded `", "`. */
const addedModsText = computed(() =>
  formatList(locale.value, unappliedActiveChanges.value?.added.map(modLabel.label) ?? []),
);
const removedModsText = computed(() =>
  formatList(locale.value, unappliedActiveChanges.value?.removed.map(modLabel.label) ?? []),
);

const problemItems = computed(() => preflight.value?.items ?? []);
const unansweredProblemCount = computed(
  () => problemItems.value.filter((item) => !item.acknowledged).length,
);

/**
 * How many distinct suggested reorders from the last verify run have
 * neither been turned into a pair rule from this dialog
 * ({@link createdPairRules}) nor already existed as one when the report
 * was produced ({@link existingUserPairKeys}) — an inline warning, never
 * a notification: the verify report is `ref` state local to this
 * dialog, not app-global fact `evaluate()` could read. Never blocks
 * Apply — purely informational.
 */
const unansweredReorderCount = computed(() => {
  const pairKeys = new Set<string>();
  for (const row of [...orderFixableOperations.value, ...partlyFixableOperations.value]) {
    for (const reorder of row.reorders) {
      const pairKey = pairKeyOf(reorder);
      if (createdPairRules.value.has(pairKey) || existingUserPairKeys.value.has(pairKey)) {
        continue;
      }
      pairKeys.add(pairKey);
    }
  }
  return pairKeys.size;
});
</script>

<template>
  <Message
    v-if="unansweredReorderCount > 0"
    severity="warn"
    :closable="false"
    data-testid="apply-dialog-unanswered-reorders-warning"
  >
    {{ t("apply.preflight.unansweredReordersWarning", { count: unansweredReorderCount }, unansweredReorderCount) }}
  </Message>

  <div
    v-if="hasUnappliedActiveChanges && unappliedActiveChanges"
    class="border-border-subtle flex flex-col gap-1 rounded border p-2 text-xs"
    data-testid="apply-dialog-unapplied-active-changes"
  >
    <p class="text-text-muted">
      {{ t("apply.preflight.unappliedHeading") }}
    </p>
    <p v-if="unappliedActiveChanges.added.length > 0">
      {{ t("apply.preflight.addedLabel") }}
      {{ addedModsText }}
    </p>
    <p v-if="unappliedActiveChanges.removed.length > 0">
      {{ t("apply.preflight.removedLabel") }}
      {{ removedModsText }}
    </p>
  </div>

  <details
    v-if="preflight"
    class="border-border-subtle rounded border p-2 text-sm"
    data-testid="apply-dialog-problems"
  >
    <summary
      class="text-text cursor-pointer"
      data-testid="apply-dialog-problems-summary"
    >
      <template v-if="problemItems.length === 0">
        {{ t("apply.preflight.noProblems") }}
      </template>
      <template v-else-if="unansweredProblemCount > 0">
        {{ t("apply.preflight.problemsRow", { count: unansweredProblemCount }, unansweredProblemCount) }}
      </template>
      <template v-else>
        {{ t("apply.preflight.problemsAllDecided", { count: problemItems.length }, problemItems.length) }}
      </template>
    </summary>
    <ApplyProblemList
      v-if="problemItems.length > 0"
      class="mt-2"
      :items="problemItems"
    />
  </details>

  <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
    <dt class="text-text-muted">
      {{ t("apply.preflight.orderLabel") }}
    </dt>
    <dd data-testid="apply-dialog-order-source">
      {{ tm(orderSourceLabel(session.selected)) }}
    </dd>
    <dt class="text-text-muted">
      {{ t("apply.preflight.modsConfigLabel") }}
    </dt>
    <dd
      class="break-all"
      data-testid="apply-dialog-mods-config-path"
    >
      {{ modsConfigPath }}
    </dd>
    <dt class="text-text-muted">
      {{ t("apply.preflight.decisionsLabel") }}
    </dt>
    <dd
      class="break-all"
      data-testid="apply-dialog-decisions-path"
    >
      {{ decisionsPath }}
    </dd>
    <dt class="text-text-muted">
      {{ t("apply.preflight.rulesLabel") }}
    </dt>
    <dd
      class="break-all"
      data-testid="apply-dialog-rules-path"
    >
      {{ rulesPath }}
    </dd>
    <dt class="text-text-muted">
      {{ t("apply.preflight.movedModsLabel") }}
    </dt>
    <dd>
      <span data-testid="apply-dialog-moved-mods">{{ movedMods }}</span>
      <span
        class="text-text-faint block text-xs"
        data-testid="apply-dialog-sort-provenance"
      >
        {{ sortProvenance }}
      </span>
    </dd>
    <dt class="text-text-muted">
      {{ t("apply.preflight.needsInputLabel") }}
    </dt>
    <dd>
      <span data-testid="apply-dialog-needs-input">{{ needsInput }}</span>
      <RouterLink
        v-if="needsInput > 0"
        to="/inbox"
        class="ml-2 text-xs underline"
        data-testid="apply-dialog-inbox-link"
        @click="close"
      >
        {{ t("apply.preflight.openInbox") }}
      </RouterLink>
    </dd>
  </dl>
</template>
