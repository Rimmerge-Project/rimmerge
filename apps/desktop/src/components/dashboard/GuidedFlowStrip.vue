<script setup lang="ts">
// The Dashboard's four-step path to the suggested order. Every state is derived from session
// facts (see `utils/guidedFlow.ts`); only whether this strip's own dialog is open is local.
import Button from "primevue/button";
import { computed, nextTick, ref, useId, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink } from "vue-router";
import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import GuidedRulesActions from "@/components/dashboard/GuidedRulesActions.vue";
import GuidedRulesBody from "@/components/dashboard/GuidedRulesBody.vue";
import { useGuidedRulesStep } from "@/composables/useGuidedRulesStep";
import { useOrderSource } from "@/composables/useOrderSource";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery, useRescanMutation } from "@/queries/activeSet";
import type { DashboardDto } from "@/types/generated/DashboardDto";
import { assertNever } from "@/utils/assertNever";
import { guidedStep, type StepStatus, stepStatuses } from "@/utils/guidedFlow";
import { describeRecommendedRulesProgress } from "@/utils/recommendedRules";

const { data } = defineProps<{ data: DashboardDto }>();

const { t } = useI18n();
const tm = useTranslateMessage();
const { select, isSwitching } = useOrderSource();
const { data: pendingActiveChanges } = usePendingActiveChangesQuery();
const { mutate: rescan, isLoading: rescanning } = useRescanMutation();

const {
  rules,
  hasLoadFailed: hasRulesLoadFailed,
  isSkipping,
  progress: rulesProgress,
  runLabel: rulesRunLabel,
  getRules,
  skip: skipRules,
} = useGuidedRulesStep();

const headingId = useId();
const root = useTemplateRef<HTMLElement>("root");
const dialogOpen = ref(false);

const isStaleActiveSet = computed(() => {
  const diff = pendingActiveChanges.value?.unscanned;
  return diff !== undefined && (diff.added.length > 0 || diff.removed.length > 0);
});

const step = computed(() =>
  guidedStep({
    rules: rules.value,
    selected: data.selected,
    fileMatchesSuggested: data.fileMatchesSuggested,
    isStaleActiveSet: isStaleActiveSet.value,
    dialogOpen: dialogOpen.value,
  }),
);
const statuses = computed(() => stepStatuses(step.value, rules.value));

/** The selected order's needs-input count, shown as information only. */
const needsInputCount = computed(() => data.ledgerStats[data.selected].needsInput);

function statusLabel(status: StepStatus): string {
  switch (status) {
    case "done":
      return t("dashboard.guide.status.done");
    case "current":
      return t("dashboard.guide.status.current");
    case "upcoming":
      return t("dashboard.guide.status.upcoming");
    case "skipped":
      return t("dashboard.guide.status.skipped");
    case "unavailable":
      return t("dashboard.guide.status.unavailable");
    default:
      return assertNever(status);
  }
}

function statusIcon(status: StepStatus): string {
  switch (status) {
    case "done":
      return "pi-check-circle text-status-auto";
    case "current":
      return "pi-arrow-circle-right text-accent";
    case "upcoming":
      return "pi-circle text-text-faint";
    case "skipped":
      return "pi-forward text-text-faint";
    case "unavailable":
      return "pi-ban text-text-faint";
    default:
      return assertNever(status);
  }
}

const rows = computed(() => [
  { id: "rules", number: 1, status: statuses.value[0] },
  { id: "choose", number: 2, status: statuses.value[1] },
  { id: "apply", number: 3, status: statuses.value[2] },
  { id: "confirm", number: 4, status: statuses.value[3] },
]);

// A function ref: the Apply button sits inside the step `v-for`, where a string ref would be an array.
let applyButtonInstance: { $el: HTMLElement } | null = null;
function setApplyButton(instance: unknown): void {
  applyButtonInstance = (instance as { $el: HTMLElement } | null) ?? null;
}

const announcement = ref("");

/** Spoken (politely) as each phase of a running "Get the recommended rules" click begins. */
const rulesAnnouncement = computed(() =>
  rulesProgress.value === null ? "" : tm(describeRecommendedRulesProgress(rulesProgress.value)),
);

/**
 * Moves focus to the current step's primary control once step 1 settled (the control that
 * had focus is gone or replaced); with no control left (everything done), to step 2's row.
 */
async function focusNextControl(): Promise<void> {
  await nextTick();
  if (step.value.kind === "getRules" || step.value.kind === "confirm") {
    return;
  }
  const primary = root.value?.querySelector<HTMLElement>(
    '[data-testid="guide-choose-button"], [data-testid="guide-rescan-button"], [data-testid="dashboard-apply-button"]',
  );
  const fallback = root.value?.querySelector<HTMLElement>('[data-testid="guide-step-choose"]');
  (primary ?? fallback)?.focus();
}

/**
 * After a click that did not finish step 1, focus returns to the control that offers a retry;
 * when the step became unavailable there is none, so it falls to Skip rather than `<body>`.
 */
async function focusRulesControl(): Promise<void> {
  await nextTick();
  const retry = root.value?.querySelector<HTMLElement>(
    '[data-testid="guide-rules-button"], [data-testid="guide-rules-get-now"]',
  );
  const skip = root.value?.querySelector<HTMLElement>('[data-testid="guide-rules-skip"]');
  (retry ?? skip)?.focus();
}

async function getRulesNow(): Promise<void> {
  await getRules();
  if (rules.value?.kind === "done") {
    await focusNextControl();
    return;
  }
  await focusRulesControl();
}

async function skipRulesNow(): Promise<void> {
  await skipRules();
  if (rules.value?.kind === "skipped") {
    await focusNextControl();
  }
}

// After "Use suggested order" lands, focus follows to step 2's Apply button.
const focusApplyAfterChoose = ref(false);
function chooseSuggested(): void {
  focusApplyAfterChoose.value = true;
  select("suggested");
}

watch(
  () => step.value.kind,
  async (kind, previous) => {
    if (kind === "apply" && previous === "chooseSuggested" && focusApplyAfterChoose.value) {
      await nextTick();
      applyButtonInstance?.$el.focus();
    }
    focusApplyAfterChoose.value = false;
    // Spoken only on a transition into done, never on first render.
    announcement.value =
      kind === "done" && previous !== "done" ? t("dashboard.guide.appliedAnnouncement") : "";
  },
);
</script>

<template>
  <section
    ref="root"
    class="surface-card flex flex-col gap-3 p-4"
    :aria-labelledby="headingId"
    data-testid="guided-flow-strip"
  >
    <div>
      <h2
        :id="headingId"
        class="text-text text-base font-semibold"
      >
        {{ t("dashboard.guide.heading") }}
      </h2>
      <p class="text-text-muted text-sm">
        {{ t("dashboard.guide.intro") }}
      </p>
    </div>

    <ol
      class="flex flex-col gap-2"
      :aria-label="t('dashboard.guide.listLabel')"
    >
      <li
        v-for="row in rows"
        :key="row.id"
        :aria-current="row.status === 'current' ? 'step' : undefined"
        :class="[
          'border-border-subtle flex flex-wrap items-start justify-between gap-3 rounded border p-3',
          row.status === 'current' ? 'bg-surface-2' : '',
        ]"
        :data-testid="`guide-step-${row.id}`"
        :data-status="row.status"
        :tabindex="row.id === 'choose' ? -1 : undefined"
      >
        <div class="flex min-w-0 flex-1 items-start gap-3">
          <span
            class="text-text-muted w-4 shrink-0 text-sm font-semibold"
          >{{ row.number }}</span>
          <div class="flex min-w-0 flex-col gap-0.5">
            <div class="flex items-center gap-2">
              <span class="text-text text-sm font-medium">
                <template v-if="row.id === 'rules'">{{ t("dashboard.guide.getRules.title") }}</template>
                <template v-else-if="row.id === 'choose'">{{ t("dashboard.guide.chooseSuggested.title") }}</template>
                <template v-else-if="row.id === 'apply'">{{ t("dashboard.guide.apply.title") }}</template>
                <template v-else>{{ t("dashboard.guide.confirm.title") }}</template>
              </span>
              <span class="text-text-muted flex items-center gap-1 text-xs">
                <i
                  :class="['pi text-xs', statusIcon(row.status)]"
                  aria-hidden="true"
                />
                {{ statusLabel(row.status) }}
              </span>
            </div>

            <GuidedRulesBody
              v-if="row.id === 'rules'"
              :rules="rules"
              :progress="rulesProgress"
              :has-load-failed="hasRulesLoadFailed"
            />

            <template v-else-if="row.id === 'choose'">
              <p class="text-text-muted text-sm">
                <template v-if="row.status === 'done' && data.selected === 'suggested'">
                  {{ t("dashboard.guide.chooseSuggested.doneText") }}
                </template>
                <template v-else-if="row.status === 'done'">
                  {{ t("dashboard.guide.chooseSuggested.doneMatchesFileText") }}
                </template>
                <template v-else>
                  {{ t("dashboard.guide.chooseSuggested.pending", { count: data.movedMods }, data.movedMods) }}
                </template>
              </p>
              <RouterLink
                to="/order"
                class="text-sm underline"
                data-testid="guide-review-link"
              >
                {{ t("dashboard.guide.chooseSuggested.reviewLink") }}
              </RouterLink>
            </template>

            <template v-else-if="row.id === 'apply'">
              <p
                v-if="step.kind === 'apply' && step.blockedByStaleActiveSet"
                class="text-text-muted text-sm"
                data-testid="guide-stale-message"
              >
                {{ t("dashboard.guide.apply.staleBlocked") }}
              </p>
              <p
                v-else-if="row.status !== 'done'"
                class="text-text-muted text-sm"
              >
                {{ t("dashboard.guide.apply.pending") }}
              </p>
            </template>

            <p
              v-else-if="row.status !== 'done'"
              class="text-text-muted text-sm"
            >
              {{ t("dashboard.guide.confirm.pending") }}
            </p>
          </div>
        </div>

        <div class="shrink-0">
          <GuidedRulesActions
            v-if="row.id === 'rules'"
            :rules="rules"
            :is-skipping="isSkipping"
            :run-label="rulesRunLabel"
            @get="getRulesNow"
            @skip="skipRulesNow"
          />
          <Button
            v-else-if="row.id === 'choose' && row.status === 'current'"
            :label="t('dashboard.guide.chooseSuggested.button')"
            :loading="isSwitching"
            data-testid="guide-choose-button"
            @click="chooseSuggested"
          />
          <template v-else-if="row.id === 'apply' && step.kind !== 'done' && step.kind !== 'confirm'">
            <Button
              v-if="step.kind === 'apply' && step.blockedByStaleActiveSet"
              :label="t('dashboard.guide.apply.rescanButton')"
              :loading="rescanning"
              data-testid="guide-rescan-button"
              @click="rescan()"
            />
            <Button
              v-else
              :ref="setApplyButton"
              :label="t('dashboard.guide.apply.button')"
              :severity="row.status === 'current' ? undefined : 'secondary'"
              data-testid="dashboard-apply-button"
              @click="dialogOpen = true"
            />
          </template>
        </div>
      </li>
    </ol>

    <p
      v-if="step.kind === 'done'"
      class="text-text text-sm"
      data-testid="guide-done"
    >
      {{ t("dashboard.guide.done") }}
      <span class="text-text-muted">{{ t("dashboard.guide.asOfLastScan") }}</span>
    </p>

    <p
      v-if="needsInputCount > 0"
      class="text-text-muted text-sm"
      data-testid="guide-needs-input"
    >
      {{ t("dashboard.guide.needsInputHint", { count: needsInputCount }, needsInputCount) }}
      ·
      <RouterLink
        to="/inbox"
        class="underline"
        data-testid="guide-inbox-link"
      >
        {{ t("dashboard.guide.openInbox") }}
      </RouterLink>
    </p>

    <div
      class="sr-only"
      aria-live="polite"
      data-testid="guide-live-region"
    >
      {{ announcement }}
    </div>

    <div
      class="sr-only"
      aria-live="polite"
      data-testid="guide-rules-live-region"
    >
      {{ rulesAnnouncement }}
    </div>

    <ApplyDialog v-model:visible="dialogOpen" />
  </section>
</template>
