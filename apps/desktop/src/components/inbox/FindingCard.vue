<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import BaseConfidence from "@/components/base/BaseConfidence.vue";
import BaseStatusPill from "@/components/base/BaseStatusPill.vue";
import ScopeBadge from "@/components/patches/ScopeBadge.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";
import { edgeKindLabel } from "@/utils/edgeKind";
import { describesFindingWhenAccepted } from "@/utils/finding";
import { findingTitle } from "@/utils/findingTitle";
import {
  describeAction,
  describeMergeState,
  mergeStateBadgeClasses,
  primaryModIdOfAction,
} from "@/utils/format";

const {
  resolution,
  active = false,
  resolvedBySuggested = null,
} = defineProps<{
  resolution: ResolutionSummaryDto;
  /** Whether this is the row the keyboard cursor is on. */
  active?: boolean;
  /**
   * Only meaningful with `Current` selected: would the suggested order
   * already resolve this finding? `null` when not applicable (the
   * suggested order is selected, or the backend didn't compute it).
   */
  resolvedBySuggested?: boolean | null;
}>();
const emit = defineEmits<{ select: [] }>();

const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();
/**
 * `resolution.scope` is non-`null` only in a compat patch's own scoped
 * inbox; an undecided entry
 * there has its suggestion rewritten to `Ignore` — informative for the
 * patch's own bookkeeping, meaningless as a card title, since every
 * undecided card in a patch would then read "Ignore".
 */
const isUndecidedInPatch = computed(() => resolution.scope !== null && !resolution.hasDecision);
/** The finding's own title, derived from its evidence — see {@link findingTitle}. */
const title = computed(() => tm(findingTitle(resolution.finding, t, modLabel.label)));
/**
 * A bare `Accept` (the only action `ledger::suggest` ever recommends for
 * `runtimePatchCollision`/`ruleOverruled`/`placementOverruled`/
 * `placementQuestioned`) names no mod or def of its own — showing it as
 * the card's title would read "Accept" for every one of them, so those
 * four fall back to the finding-key description instead while still
 * undecided (or decided as the suggested `Accept`). Once a real decision
 * is made (`Reorder`, `PromoteRule`, ...), `describeAction` takes back
 * over — that action does describe what happened.
 */
const showsFindingDescription = computed(
  () =>
    isUndecidedInPatch.value ||
    (resolution.effective.kind === "accept" && describesFindingWhenAccepted(resolution.key)),
);
const actionText = computed(() =>
  showsFindingDescription.value
    ? title.value
    : tm(describeAction(resolution.effective, modLabel.label, (kind) => tm(edgeKindLabel(kind)))),
);
/**
 * A `title` for {@link actionText}: the mod's id when the action names
 * exactly one mod, otherwise the finding's own description — a bare
 * `Accept` title (every `runtimePatchCollision`/`ruleOverruled`/... row)
 * names nothing on its own, so hover is the only place the row can say
 * what it is about without redesigning it.
 */
const actionTitle = computed(() => {
  if (showsFindingDescription.value) {
    return undefined;
  }
  const id = primaryModIdOfAction(resolution.effective);
  return id === null ? title.value : modLabel.titleFor(id);
});
</script>

<template>
  <button
    type="button"
    class="border-border-subtle hover:bg-surface-2 flex w-full cursor-pointer flex-col gap-1 border-b border-l-2 border-l-transparent px-3 py-2 text-left text-sm focus-visible:outline"
    :class="active ? 'bg-accent-soft border-l-accent' : ''"
    :aria-current="active"
    :data-testid="`finding-card-${resolution.key}`"
    @click="emit('select')"
  >
    <div class="flex items-center justify-between gap-2">
      <span class="flex min-w-0 items-center gap-1.5">
        <span
          class="text-text line-clamp-2 font-medium break-words"
          :title="actionTitle"
          data-testid="card-action-text"
        >{{ actionText }}</span>
        <span
          v-if="isUndecidedInPatch"
          class="text-text-muted shrink-0 text-xs"
          data-testid="card-undecided-label"
        >{{ t("inbox.findingCard.undecided") }}</span>
      </span>
      <span class="flex shrink-0 items-center gap-1">
        <ScopeBadge
          v-if="resolution.scope?.kind === 'partial'"
          :outside="resolution.scope.outside"
        />
        <span
          v-if="resolution.mergeState"
          class="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium whitespace-nowrap"
          :class="mergeStateBadgeClasses(resolution.mergeState)"
          data-testid="merge-state-pill"
        >
          {{ tm(describeMergeState(resolution.mergeState, resolution.structuralGuardField)) }}
        </span>
        <BaseStatusPill :status="resolution.status" />
      </span>
    </div>
    <div class="flex items-center justify-between gap-2">
      <span
        class="text-text-faint truncate font-mono text-[11px]"
        :title="resolution.key"
      >{{
        resolution.key
      }}</span>
      <BaseConfidence :confidence="resolution.confidence" />
    </div>
    <div class="flex gap-1">
      <span
        v-if="resolution.hasDecision"
        class="bg-surface-2 text-text-muted rounded px-1.5 py-0.5 text-xs"
        data-testid="finding-card-decided-chip"
      >
        {{ t("inbox.findingCard.decidedChip") }}
      </span>
      <span
        v-if="resolvedBySuggested"
        class="bg-status-auto-soft text-status-auto rounded px-1.5 py-0.5 text-xs"
        data-testid="resolved-by-suggested-chip"
      >
        {{ t("inbox.findingCard.resolvedBySuggestedChip") }}
      </span>
    </div>
  </button>
</template>
