<script setup lang="ts">
import Button from "primevue/button";
import Message from "primevue/message";
import Tab from "primevue/tab";
import TabList from "primevue/tablist";
import Tabs from "primevue/tabs";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import AddPairRuleDialog from "@/components/rules/AddPairRuleDialog.vue";
import AddPlacementRuleDialog from "@/components/rules/AddPlacementRuleDialog.vue";
import ImportRimSortDialog from "@/components/rules/ImportRimSortDialog.vue";
import OrphanedDecisions from "@/components/rules/OrphanedDecisions.vue";
import RuleDatabasesCard from "@/components/rules/RuleDatabasesCard.vue";
import RuleTable from "@/components/rules/RuleTable.vue";
import TagRuleEditor from "@/components/rules/TagRuleEditor.vue";
import { useRevertDecisionMutation } from "@/queries/findings";
import {
  useDeleteRuleMutation,
  useImportRimSortMutation,
  useOrphanedDecisionsQuery,
  usePlacementDependentCountsQuery,
  usePromoteImportedRuleMutation,
  useRuleDatabasesQuery,
  useRulesQuery,
  useUpsertRuleMutation,
} from "@/queries/rules";
import { useSettingsQuery } from "@/queries/settings";
import { useSetManualTagMutation, useTagsQuery } from "@/queries/tags";
import { asFindingKey } from "@/types/brands";
import type { DeleteRuleRequestDto } from "@/types/generated/DeleteRuleRequestDto";
import type { ImportReportDto } from "@/types/generated/ImportReportDto";
import type { RimSortPathsDto } from "@/types/generated/RimSortPathsDto";
import type { RuleDto } from "@/types/generated/RuleDto";
import type { RuleKeyDto } from "@/types/generated/RuleKeyDto";
import type { RuleOriginDto } from "@/types/generated/RuleOriginDto";
import type { SetManualTagRequestDto } from "@/types/generated/SetManualTagRequestDto";

interface RulesTab {
  value: string;
  labelKey: string;
  origin: RuleOriginDto | null;
}

/** A literal `en.json` key per tab, resolved by `t()` at render time — never a label baked in once here. */
const TABS: RulesTab[] = [
  { value: "all", labelKey: "rules.page.tabAll", origin: null },
  { value: "userDecision", labelKey: "rules.page.tabUserDecision", origin: "userDecision" },
  { value: "rimSortUser", labelKey: "rules.page.tabRimSortUser", origin: "rimSortUser" },
  {
    value: "rimSortCommunity",
    labelKey: "rules.page.tabRimSortCommunity",
    origin: "rimSortCommunity",
  },
  { value: "steamDb", labelKey: "rules.page.tabSteamDb", origin: "steamDb" },
];

const { t } = useI18n();
const activeTab = ref("all");
const activeOrigin = computed(
  () => TABS.find((tab) => tab.value === activeTab.value)?.origin ?? null,
);

const { data: rules, isPending, error } = useRulesQuery(activeOrigin);
const { data: settings } = useSettingsQuery();
const { data: tagging } = useTagsQuery();
const { data: orphaned } = useOrphanedDecisionsQuery();

const { mutate: deleteRule } = useDeleteRuleMutation();
const { mutate: promoteRule } = usePromoteImportedRuleMutation();
const { mutate: setManualTag } = useSetManualTagMutation();
const { mutateAsync: revertDecision } = useRevertDecisionMutation();
const { mutateAsync: importRimSort } = useImportRimSortMutation();
const { mutateAsync: upsertRule, isLoading: isUpsertingRule } = useUpsertRuleMutation();

const importDialogVisible = ref(false);
const addRuleDialogVisible = ref(false);
const addPairRuleDialogVisible = ref(false);

// `PlacementPromotesDependents` only exists once a mod's placement rule
// is *already* in effect (the ledger walks the current sort outcome to
// compute it) — there is no way to preview a pin's blast radius *before*
// saving it without new backend work, disclosed rather than built. See
// `usePlacementDependentCountsQuery`'s own doc comment for how this is
// still reachable with zero new command: every current pin's own count
// is fetched once here and shown on every placement row (new and
// pre-existing alike) — the closest honest substitute for a pre-commit
// preview, and arguably more useful, since it also answers the question
// for pins the user never added themselves.
const { data: dependentCounts } = usePlacementDependentCountsQuery();

// Read here too (Pinia Colada dedupes against `RuleDatabasesCard.vue`'s
// own copy of the same query, so this is not a second fetch) purely to
// compute the inline re-import badge next to the import button — the
// card itself still owns the full per-source detail below.
const { data: ruleDatabases } = useRuleDatabasesQuery();
const anyDatabaseNeedsReimport = computed(
  () => ruleDatabases.value?.some((view) => view.needsReimport) ?? false,
);

// The migration-warning message (`rules.json`'s dropped-cluster-rules
// notice) has no way to stop being true once it's happened — dismissing
// it is purely a viewing preference, tracked locally rather than in any
// query cache, and lost on reload same as any other transient UI state.
const dismissedWarnings = ref(new Set<string>());
function dismissWarning(warning: string): void {
  dismissedWarnings.value = new Set(dismissedWarnings.value).add(warning);
}
const visibleWarnings = computed(() =>
  (rules.value?.warnings ?? []).filter((warning) => !dismissedWarnings.value.has(warning)),
);

// Both toggles live on the load-order/settings page, not here — the
// rules page only states their current state next to the import button
// and greys the rows they affect, so an import is never silently
// ignored. Defaults match `Settings::default` for the brief window
// before the settings query resolves.
const useImportedPairs = computed(() => settings.value?.useImportedPairs ?? false);
const useImportedPlacements = computed(() => settings.value?.useImportedPlacements ?? true);

function handleDelete(request: DeleteRuleRequestDto): void {
  deleteRule(request);
}

function handlePromote(key: RuleKeyDto): void {
  promoteRule(key);
}

function saveManualTag(request: SetManualTagRequestDto): void {
  setManualTag(request);
}

async function pruneDecision(key: string): Promise<void> {
  await revertDecision(asFindingKey(key));
}

// Awaits the mutation so the dialog only closes once the import actually
// succeeded — a failed import leaves it open (with the mutation's own
// error surfaced by the global toast) so the user can retry instead of
// losing the paths they entered.
//
// A missing `userRules.json` must read as a warning, never as an
// imported zero. `ImportReportDto`'s three count fields are already
// `null` (not imported) vs. `0` (imported and genuinely empty) on the
// wire — this page keeps the report rather than discarding it, so that
// distinction reaches the user. `lastImportReport` is
// shown until the next import (or dismissed), same lifetime as the
// load-warning banners.
const lastImportReport = ref<ImportReportDto | null>(null);
async function runImport(paths: RimSortPathsDto): Promise<void> {
  lastImportReport.value = await importRimSort(paths);
  importDialogVisible.value = false;
}

/** `null` (not part of this import) renders distinctly from a real `0`. */
function formatImportCount(count: number | null): string {
  return count === null ? t("rules.page.notImported") : `${count}`;
}

// The banner's *severity*, not just its text, tracks whether every source
// imported, since a not-imported source must read as a warning. A report
// where every source
// actually imported (even a genuine `0`) is unremarkable, plain `info`;
// any `null` source — most importantly a missing `userRules.json` —
// escalates the whole banner to `warn`.
const importReportSeverity = computed<"info" | "warn">(() => {
  const report = lastImportReport.value;
  if (!report) return "info";
  const anyNotImported =
    report.userRules === null ||
    report.communityRules === null ||
    report.steamDependencies === null;
  return anyNotImported ? "warn" : "info";
});

/**
 * See `runImport`'s own doc comment — same "only close on success" shape.
 * Shared by both add-rule dialogs: `upsertRule` is already generic over
 * every `RuleDto` shape, so there's nothing kind-specific to branch on
 * here — only which dialog's own visibility flag to clear.
 */
async function addPlacementRule(rule: RuleDto): Promise<void> {
  await upsertRule(rule);
  addRuleDialogVisible.value = false;
}
async function addPairRule(rule: RuleDto): Promise<void> {
  await upsertRule(rule);
  addPairRuleDialogVisible.value = false;
}
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
      <h1 class="text-text shrink-0 text-xl font-semibold">
        {{ t("rules.page.heading") }}
      </h1>
      <div class="flex flex-wrap items-center justify-end gap-3">
        <p
          class="text-text-muted text-xs"
          data-testid="rules-import-toggle-state"
        >
          {{
            t("rules.page.importToggleState", {
              pairs: useImportedPairs ? t("order.sortProvenance.on") : t("order.sortProvenance.off"),
              placements: useImportedPlacements
                ? t("order.sortProvenance.on")
                : t("order.sortProvenance.off"),
            })
          }}
        </p>
        <Button
          :label="t('rules.page.addRuleButton')"
          class="shrink-0 whitespace-nowrap"
          severity="secondary"
          data-testid="open-add-rule-dialog"
          @click="addRuleDialogVisible = true"
        />
        <Button
          :label="t('rules.page.addPairRuleButton')"
          class="shrink-0 whitespace-nowrap"
          severity="secondary"
          data-testid="open-add-pair-rule-dialog"
          @click="addPairRuleDialogVisible = true"
        />
        <span
          v-if="anyDatabaseNeedsReimport"
          class="text-status-warn text-xs font-medium"
          data-testid="rule-databases-reimport-hint"
        >
          {{ t("rules.page.reimportHint") }}
        </span>
        <Button
          :label="t('rules.page.importFromRimSortButton')"
          class="shrink-0 whitespace-nowrap"
          data-testid="open-import-dialog"
          @click="importDialogVisible = true"
        />
      </div>
    </div>

    <Message
      v-if="lastImportReport"
      :severity="importReportSeverity"
      closable
      data-testid="import-report"
      @close="lastImportReport = null"
    >
      <p>{{ t("rules.page.importFinished") }}</p>
      <ul class="mt-1 list-disc pl-4">
        <li data-testid="import-report-user-rules">
          {{ t("rules.page.userRulesCount", { count: formatImportCount(lastImportReport.userRules) }) }}
        </li>
        <li data-testid="import-report-community-rules">
          {{
            t("rules.page.communityRulesCount", {
              count: formatImportCount(lastImportReport.communityRules),
            })
          }}
        </li>
        <li data-testid="import-report-steam-dependencies">
          {{
            t("rules.page.steamDependenciesCount", {
              count: formatImportCount(lastImportReport.steamDependencies),
            })
          }}
        </li>
      </ul>
    </Message>

    <Message
      v-for="warning in visibleWarnings"
      :key="warning"
      severity="warn"
      closable
      data-testid="rules-load-warning"
      @close="dismissWarning(warning)"
    >
      {{ warning }}
    </Message>

    <Tabs v-model:value="activeTab">
      <TabList>
        <Tab
          v-for="tab in TABS"
          :key="tab.value"
          :value="tab.value"
          :data-testid="`rules-tab-${tab.value}`"
        >
          {{ t(tab.labelKey) }}
        </Tab>
      </TabList>
    </Tabs>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="rules-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="rules-error"
    >
      {{ error instanceof Error ? error.message : t("rules.page.loadFailed") }}
    </p>
    <RuleTable
      v-else-if="rules"
      :rules="rules"
      :use-imported-pairs="useImportedPairs"
      :use-imported-placements="useImportedPlacements"
      :dependent-counts="dependentCounts ?? {}"
      @delete="handleDelete"
      @promote="handlePromote"
    />

    <RuleDatabasesCard />

    <section aria-labelledby="tags-heading">
      <h2
        id="tags-heading"
        class="text-text mb-2 text-base font-semibold"
      >
        {{ t("rules.page.tagsHeading") }}
      </h2>
      <TagRuleEditor
        :assignments="tagging?.assignments ?? []"
        @save="saveManualTag"
      />
    </section>

    <section aria-labelledby="orphaned-heading">
      <h2
        id="orphaned-heading"
        class="text-text mb-2 text-base font-semibold"
      >
        {{ t("rules.page.orphanedHeading") }}
      </h2>
      <OrphanedDecisions
        :decisions="orphaned ?? []"
        @prune="pruneDecision"
      />
    </section>

    <ImportRimSortDialog
      v-model:visible="importDialogVisible"
      @import="runImport"
    />
    <AddPlacementRuleDialog
      v-model:visible="addRuleDialogVisible"
      :pending="isUpsertingRule"
      :existing-placements="rules?.placements ?? []"
      @add="addPlacementRule"
    />
    <AddPairRuleDialog
      v-model:visible="addPairRuleDialogVisible"
      :pending="isUpsertingRule"
      :existing-pairs="rules?.pairs ?? []"
      @add="addPairRule"
    />
  </div>
</template>
