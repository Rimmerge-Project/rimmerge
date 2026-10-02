<script setup lang="ts">
// Step 1's text: what a click would do, the live progress while it runs, or why it is
// unavailable, skipped or done. Every sentence is its own whole key; the controls are in
// `GuidedRulesActions.vue`.
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink } from "vue-router";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { RecommendedRulesProgressEventDto } from "@/types/generated/RecommendedRulesProgressEventDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import type { SourceNeedEntryDto } from "@/types/generated/SourceNeedEntryDto";
import type { UnavailableReasonDto } from "@/types/generated/UnavailableReasonDto";
import { assertNever } from "@/utils/assertNever";
import {
  describeRecommendedRulesProgress,
  describeSourceNeed,
  describeUnavailable,
  isSteamDownload,
} from "@/utils/recommendedRules";

const { rules, progress, hasLoadFailed } = defineProps<{
  /** `null` until the step query answers. */
  rules: RecommendedRulesStepDto | null;
  progress: RecommendedRulesProgressEventDto | null;
  hasLoadFailed: boolean;
}>();

const { t, locale } = useI18n();
const tm = useTranslateMessage();

type SourceLine = { readonly database: string; readonly text: string };

/** What the body shows, decided once per state so a new DTO variant is a compile error here. */
type BodyView =
  | { readonly kind: "loading" }
  | { readonly kind: "loadFailed" }
  | { readonly kind: "offered"; readonly lines: readonly SourceLine[] }
  | { readonly kind: "inProgress" }
  | {
      readonly kind: "unavailable";
      readonly reason: UnavailableReasonDto;
      readonly message: string;
      readonly linksToSettings: boolean;
    }
  | { readonly kind: "skipped"; readonly lines: readonly SourceLine[] }
  | { readonly kind: "done"; readonly isInUse: boolean };

function sourceLines(sources: readonly SourceNeedEntryDto[]): SourceLine[] {
  return sources.map((entry) => ({
    database: entry.database,
    text: tm(describeSourceNeed(entry, locale.value)),
  }));
}

function viewOf(step: RecommendedRulesStepDto): BodyView {
  switch (step.kind) {
    case "needsAction":
      return { kind: "offered", lines: sourceLines(step.sources) };
    case "inProgress":
      return { kind: "inProgress" };
    case "unavailable": {
      const described = describeUnavailable(step.reason);
      return {
        kind: "unavailable",
        reason: step.reason,
        message: tm(described.message),
        linksToSettings: described.linksToSettings,
      };
    }
    case "skipped":
      return { kind: "skipped", lines: sourceLines(step.sources) };
    case "done":
      return { kind: "done", isInUse: step.importedRulesInUse };
    default:
      return assertNever(step);
  }
}

const view = computed<BodyView>(() => {
  if (rules !== null) {
    return viewOf(rules);
  }
  return hasLoadFailed ? { kind: "loadFailed" } : { kind: "loading" };
});
</script>

<template>
  <p
    v-if="view.kind === 'loading'"
    class="text-text-muted text-sm"
    data-testid="guide-rules-loading"
  >
    {{ t("common.loading") }}
  </p>
  <p
    v-else-if="view.kind === 'loadFailed'"
    class="text-text-muted text-sm"
    data-testid="guide-rules-load-failed"
  >
    {{ t("dashboard.guide.getRules.loadFailed") }}
  </p>

  <template v-else-if="view.kind === 'offered'">
    <p class="text-text-muted text-sm">
      {{ t("dashboard.guide.getRules.pending") }}
    </p>
    <ul
      class="text-text-muted list-disc pl-5 text-sm"
      data-testid="guide-rules-sources"
    >
      <li
        v-for="line in view.lines"
        :key="line.database"
        :data-testid="`guide-rules-source-${line.database}`"
      >
        {{ line.text }}
      </li>
    </ul>
  </template>

  <template v-else-if="view.kind === 'inProgress'">
    <p
      v-if="progress !== null"
      class="text-text-muted text-sm"
      data-testid="guide-rules-progress"
    >
      {{ tm(describeRecommendedRulesProgress(progress)) }}
    </p>
    <p
      v-else
      class="text-text-muted text-sm"
    >
      {{ t("dashboard.guide.getRules.pending") }}
    </p>
    <p
      v-if="isSteamDownload(progress)"
      class="text-text-muted text-sm"
      data-testid="guide-rules-slow-note"
    >
      {{ t("dashboard.guide.getRules.slowNote") }}
    </p>
  </template>

  <template v-else-if="view.kind === 'unavailable'">
    <p
      class="text-text-muted text-sm"
      :data-reason="view.reason"
      data-testid="guide-rules-unavailable"
    >
      {{ view.message }}
    </p>
    <RouterLink
      v-if="view.linksToSettings"
      to="/settings"
      class="text-sm underline"
      data-testid="guide-rules-settings-link"
    >
      {{ t("dashboard.guide.getRules.openSettings") }}
    </RouterLink>
  </template>

  <template v-else-if="view.kind === 'skipped'">
    <p
      class="text-text-muted text-sm"
      data-testid="guide-rules-skipped-text"
    >
      {{ t("dashboard.guide.getRules.skippedText") }}
    </p>
    <ul
      class="text-text-muted list-disc pl-5 text-sm"
      data-testid="guide-rules-sources"
    >
      <li
        v-for="line in view.lines"
        :key="line.database"
        :data-testid="`guide-rules-source-${line.database}`"
      >
        {{ line.text }}
      </li>
    </ul>
  </template>

  <p
    v-else-if="view.kind === 'done'"
    class="text-text-muted text-sm"
    data-testid="guide-rules-done-text"
  >
    <template v-if="view.isInUse">
      {{ t("dashboard.guide.getRules.doneText") }}
    </template>
    <template v-else>
      {{ t("dashboard.guide.getRules.doneNotInUseText") }}
    </template>
  </p>
</template>
