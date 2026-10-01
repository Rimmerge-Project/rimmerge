<script setup lang="ts">
// The verify report: cause counts, order-fixable rows, and the grouped remainder.
import Message from "primevue/message";
import { useI18n } from "vue-i18n";
import VerifyReorderActions from "@/components/apply/VerifyReorderActions.vue";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { orderSourceSentenceLabel } from "@/utils/orderSource";

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const {
  modLabel,
  loggedOrder,
  creatingPairRule,
  verifyReport,
  verifyOperations,
  predictedVsObserved,
  predictedVsObservedCaveats,
  verifyCounterfactualStats,
  verifyCauseCounts,
  orderFixableOperations,
  partlyFixableOperations,
  createdPairRules,
  pairRuleErrors,
  existingUserPairKeys,
  createPairRule,
  groupedByModOperations,
} = useApplyDialogState();

/**
 * The verify report's own top summary line — built here rather than
 * composed inline in the template since it pluralizes three independent
 * counts (defs checked, operations, def targets). Each count owns a whole
 * plural message and the two failure clauses are joined through the
 * locale's own list conjunction, so no counted noun is ever spliced into
 * a host sentence that has to agree with it; see `utils/format.ts`'s
 * `describeMergeModGroups` for the same composite-sentence pattern.
 */
function verifySummaryText(report: NonNullable<typeof verifyReport.value>): string {
  const failures = [
    t("apply.diff.summaryOperations", { count: report.operationsTotal }, report.operationsTotal),
    t("apply.diff.summaryTargets", { count: report.defTargetsTotal }, report.defTargetsTotal),
  ];
  return t(
    "apply.diff.summary",
    {
      count: report.defsChecked,
      source: tm(orderSourceSentenceLabel(report.source)),
      list: formatList(locale.value, failures),
    },
    report.defsChecked,
  );
}

/**
 * The counterfactual phase's own summary line, built from
 * {@link verifyCounterfactualStats}' raw counts — each clause is its own
 * pluralized message, joined through the locale's own list conjunction
 * rather than a hard-coded `", "`.
 */
function counterfactualSummaryText(
  stats: NonNullable<typeof verifyCounterfactualStats.value>,
): string {
  const parts = [
    t("apply.diff.counterfactualJobsLine", { count: stats.jobs }, stats.jobs),
    t("apply.diff.counterfactualResolvedLine", { count: stats.resolved }, stats.resolved),
    t(
      "apply.diff.counterfactualDemotedLine",
      { count: stats.demotedDeadTargets },
      stats.demotedDeadTargets,
    ),
  ];
  if (stats.skipped > 0) {
    parts.push(t("apply.diff.counterfactualSkippedLine", { count: stats.skipped }, stats.skipped));
  }
  if (stats.rejectedForRegression > 0) {
    parts.push(
      t(
        "apply.diff.counterfactualRejectedLine",
        { count: stats.rejectedForRegression },
        stats.rejectedForRegression,
      ),
    );
  }
  return t("apply.diff.counterfactualSummary", { list: formatList(locale.value, parts) });
}

/** One `groupedByModOperations` row's own "N operations across M def targets" line. */
function groupedLineText(group: (typeof groupedByModOperations.value)[number]): string {
  const counts = [
    t("apply.diff.groupedOperations", { count: group.operations.length }, group.operations.length),
    t("apply.diff.groupedTargets", { count: group.defTargetCount }, group.defTargetCount),
  ];
  return t("apply.diff.groupedByModLine", {
    mod: modLabel.label(group.modId),
    list: formatList(locale.value, counts),
  });
}
</script>

<template>
  <div
    v-if="verifyReport"
    class="flex flex-col gap-2"
    data-testid="apply-dialog-verify-report"
  >
    <p
      class="text-text-muted text-xs"
      data-testid="apply-dialog-verify-summary"
    >
      {{ verifySummaryText(verifyReport) }}
    </p>

    <p
      v-if="verifyCounterfactualStats"
      class="text-text-faint text-xs"
      data-testid="apply-dialog-verify-counterfactual"
    >
      {{ counterfactualSummaryText(verifyCounterfactualStats) }}
    </p>

    <ul
      v-if="verifyOperations.length > 0"
      class="flex flex-wrap gap-x-3 gap-y-1 text-xs"
      data-testid="apply-dialog-verify-cause-counts"
    >
      <li v-if="verifyCauseCounts.removedBy > 0">
        {{ t("apply.diff.removedByCount", { count: verifyCauseCounts.removedBy }) }}
      </li>
      <li v-if="verifyCauseCounts.notYetInjected > 0">
        {{ t("apply.diff.notYetInjectedCount", { count: verifyCauseCounts.notYetInjected }) }}
      </li>
      <li v-if="verifyCauseCounts.deadTarget > 0">
        {{ t("apply.diff.deadTargetCount", { count: verifyCauseCounts.deadTarget }) }}
      </li>
      <li v-if="verifyCauseCounts.unknown > 0">
        {{ t("apply.diff.unknownCount", { count: verifyCauseCounts.unknown }) }}
      </li>
    </ul>

    <div
      v-if="orderFixableOperations.length > 0"
      data-testid="apply-dialog-verify-order-fixable"
    >
      <p class="text-text-muted text-xs">
        {{ t("apply.diff.orderFixableHeading") }}
      </p>
      <ul class="flex flex-col gap-2">
        <li
          v-for="row in orderFixableOperations"
          :key="row.key"
          class="text-xs"
        >
          <i18n-t
            keypath="apply.diff.modPossessive"
            tag="span"
          >
            <template #mod>
              {{ modLabel.label(row.operation.modId) }}
            </template>
            <template #operation>
              <code>{{ row.operation.operation }}</code>
            </template>
          </i18n-t>
          <ul class="flex flex-col gap-0.5 pl-3">
            <li
              v-for="(def, index) in row.operation.defs"
              :key="index"
              :class="{ 'text-text-faint': def.reorderKind?.kind === 'cosmetic' }"
              :data-testid="
                def.reorderKind?.kind === 'cosmetic'
                  ? 'apply-dialog-verify-cosmetic-row'
                  : undefined
              "
            >
              <code>{{ def.defKey.defType }}/{{ def.defKey.defName }}</code> —
              <template v-if="def.reorderKind?.kind === 'cosmetic'">
                {{ t("apply.diff.cosmeticText") }}
                <i18n-t
                  v-if="def.reorderKind.existingMergeKey"
                  keypath="apply.diff.mergeInstead"
                  tag="span"
                >
                  <template #link>
                    <RouterLink
                      class="underline"
                      :to="{
                        name: 'merge-editor',
                        params: { key: def.reorderKind.existingMergeKey },
                      }"
                    >
                      {{ t("apply.diff.mergeInsteadLink") }}
                    </RouterLink>
                  </template>
                </i18n-t>
                <template v-else>
                  {{ t("apply.diff.noMergeAvailable") }}
                </template>
              </template>
              <template v-else-if="def.cause.kind === 'removedBy'">
                {{ t("apply.diff.removedByCause", { mod: modLabel.label(def.cause.modId) }) }}
              </template>
              <template v-else-if="def.cause.kind === 'notYetInjected'">
                {{
                  t("apply.diff.notYetInjectedCause", { mod: modLabel.label(def.cause.modId) })
                }}
              </template>
            </li>
          </ul>
          <VerifyReorderActions
            :row-key="row.key"
            :reorders="row.reorders"
            :created-by="createdPairRules"
            :existing-rules="existingUserPairKeys"
            :errors="pairRuleErrors"
            :pending="creatingPairRule"
            @create="createPairRule(row, $event)"
          />
        </li>
      </ul>
    </div>

    <div
      v-if="groupedByModOperations.length > 0"
      data-testid="apply-dialog-verify-grouped"
    >
      <p class="text-text-muted text-xs">
        {{ t("apply.diff.notFullyFixableHeading") }}
      </p>
      <ul class="flex flex-col gap-1">
        <li
          v-for="group in groupedByModOperations"
          :key="group.modId"
          class="text-xs"
          :data-testid="`apply-dialog-verify-group-${group.modId}`"
        >
          {{ groupedLineText(group) }}
        </li>
      </ul>

      <div
        v-if="partlyFixableOperations.length > 0"
        class="pt-1"
        data-testid="apply-dialog-verify-partly-fixable"
      >
        <p class="text-text-muted text-xs">
          {{ t("apply.diff.partlyFixableHeading") }}
        </p>
        <ul class="flex flex-col gap-2">
          <li
            v-for="row in partlyFixableOperations"
            :key="row.key"
            class="text-xs"
          >
            <i18n-t
              keypath="apply.diff.modPossessive"
              tag="span"
            >
              <template #mod>
                {{ modLabel.label(row.operation.modId) }}
              </template>
              <template #operation>
                <code>{{ row.operation.operation }}</code>
              </template>
            </i18n-t>
            <VerifyReorderActions
              :row-key="row.key"
              :reorders="row.reorders"
              :created-by="createdPairRules"
              :existing-rules="existingUserPairKeys"
              :errors="pairRuleErrors"
              :pending="creatingPairRule"
              @create="createPairRule(row, $event)"
            />
          </li>
        </ul>
      </div>
    </div>

    <p
      v-if="verifyReport.skipped.length > 0"
      class="text-text-faint text-xs"
      data-testid="apply-dialog-verify-skipped"
    >
      {{
        t(
          "apply.diff.skippedCandidates",
          { count: verifyReport.skipped.length },
          verifyReport.skipped.length,
        )
      }}
    </p>

    <div
      v-if="predictedVsObserved"
      class="border-border-subtle flex flex-col gap-2 border-t pt-2"
      data-testid="apply-dialog-predicted-vs-observed"
    >
      <Message
        v-if="loggedOrder.differs.value"
        severity="warn"
        data-testid="apply-dialog-pvo-order-mismatch-warning"
      >
        {{ t("apply.diff.pvoMismatchWarning") }}
      </Message>

      <Message
        v-for="caveat in predictedVsObservedCaveats"
        :key="caveat.key"
        severity="warn"
        :data-testid="`apply-dialog-${caveat.key.split('.').pop()}`"
      >
        {{ tm(caveat) }}
      </Message>

      <p class="text-text font-medium">
        {{ t("apply.diff.pvoHeading") }}
      </p>

      <div v-if="predictedVsObserved.matched.length > 0">
        <p
          class="text-text-muted text-xs"
          data-testid="apply-dialog-pvo-matched-summary"
        >
          {{ t("apply.diff.pvoMatchedSummary", { count: predictedVsObserved.matched.length }) }}
        </p>
        <ul class="flex flex-col gap-0.5 pl-3 text-xs">
          <li
            v-for="entry in predictedVsObserved.matched"
            :key="`${entry.modId}|${entry.operation}`"
            :data-testid="`apply-dialog-pvo-matched-${entry.modId}`"
          >
            {{ modLabel.label(entry.modId) }} — <code>{{ entry.operation }}</code>
          </li>
        </ul>
      </div>

      <div v-if="predictedVsObserved.predictedOnly.length > 0">
        <p
          class="text-text-muted text-xs"
          data-testid="apply-dialog-pvo-predicted-only-summary"
        >
          {{
            t("apply.diff.pvoPredictedOnlySummary", {
              count: predictedVsObserved.predictedOnly.length,
            })
          }}
        </p>
        <ul class="flex flex-col gap-0.5 pl-3 text-xs">
          <li
            v-for="entry in predictedVsObserved.predictedOnly"
            :key="`${entry.modId}|${entry.operation}`"
            :data-testid="`apply-dialog-pvo-predicted-only-${entry.modId}`"
          >
            {{ modLabel.label(entry.modId) }} — <code>{{ entry.operation }}</code>
          </li>
        </ul>
      </div>

      <div v-if="predictedVsObserved.observedOnly.length > 0">
        <p
          class="text-text-muted text-xs"
          data-testid="apply-dialog-pvo-observed-only-summary"
        >
          {{
            t("apply.diff.pvoObservedOnlySummary", {
              count: predictedVsObserved.observedOnly.length,
            })
          }}
        </p>
        <ul class="flex flex-col gap-0.5 pl-3 text-xs">
          <li
            v-for="(entry, index) in predictedVsObserved.observedOnly"
            :key="index"
            data-testid="apply-dialog-pvo-observed-only-entry"
          >
            {{
              entry.attribution.kind === "mod"
                ? modLabel.label(entry.attribution.modId)
                : t("apply.diff.unattributedRaw", { raw: entry.attribution.raw })
            }}
            — <code>{{ entry.operation }}</code>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
