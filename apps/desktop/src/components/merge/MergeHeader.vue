<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import type { MergePreviewDto } from "@/types/generated/MergePreviewDto";
import { caveatLabel } from "@/utils/caveat";
import { describeMergeState, inspectRoute, mergeStateBadgeClasses } from "@/utils/format";

const { preview } = defineProps<{ preview: MergePreviewDto }>();
const { t, locale } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

/** "Out of scope, not merged: A, B" — every owner `preview.outOfScopeOwners` names, resolved through {@link useModLabel}. `""` (never rendered) when the list is empty — a profile preview, or a patch preview whose scope already covers every owner. */
const outOfScopeLine = computed(() => {
  if (preview.outOfScopeOwners.length === 0) {
    return "";
  }
  return t("merge.header.outOfScope", {
    mods: formatList(
      locale.value,
      preview.outOfScopeOwners.map((modId) => modLabel.label(modId)),
    ),
  });
});

/**
 * The structural guard: one banner naming the field that triggered it
 * and the owner whose contribution differs from the def's base copy;
 * the winner stays preselected. `""` (never rendered) when
 * `preview.structuralGuard` is `null` — every `patchCollision` preview,
 * and a `defOverride` preview the guard never fired for.
 *
 * **Points at the inbox, not "below"**:
 * the worked guarded case (`BionicHeart`-shaped, every field auto-resolved)
 * has zero `Conflict` rows and nothing to click in this editor at all — a
 * choice made here would only ever persist an `Action::Merge` the guard
 * holds at `NeedsFieldInput` forever (`render.skipped` on every apply).
 * The action that actually exists is accepting this finding from the
 * inbox (whatever the ledger's own suggestion is — `Accept`/`PreferWinner`,
 * never `Merge` for a guarded def), so the banner names that instead of
 * a control this page may not have.
 */
const structuralGuardLine = computed(() => {
  const guard = preview.structuralGuard;
  if (!guard) {
    return "";
  }
  return t("merge.header.structuralGuardLine", {
    field: guard.field,
    by: modLabel.label(guard.by),
  });
});
</script>

<template>
  <div
    class="border-border-subtle flex flex-col gap-2 border-b p-4"
    data-testid="merge-header"
  >
    <div class="flex items-center justify-between gap-2">
      <h1
        class="text-text text-lg font-semibold"
        data-testid="merge-header-def-key"
      >
        {{ preview.defKey.defType }}/{{ preview.defKey.defName }}
      </h1>
      <div class="flex shrink-0 items-center gap-2">
        <RouterLink
          :to="inspectRoute(preview.defRef)"
          class="text-accent text-xs underline"
          data-testid="merge-header-inspect-link"
        >
          {{ t("merge.header.inspectLink") }}
        </RouterLink>
        <span
          class="rounded-full px-2 py-0.5 text-xs font-medium"
          :class="mergeStateBadgeClasses(preview.state)"
          data-testid="merge-header-state"
        >
          {{ tm(describeMergeState(preview.state, preview.structuralGuard?.field)) }}
        </span>
      </div>
    </div>

    <div
      class="flex flex-wrap items-center gap-2"
      data-testid="merge-header-owners"
    >
      <span
        v-for="owner in preview.owners"
        :key="owner.modId"
        class="border-border-subtle text-text-muted rounded border px-2 py-0.5 text-xs"
        :class="owner.modId === preview.winner ? 'border-accent text-accent font-medium' : ''"
        :data-testid="`merge-owner-${owner.modId}`"
        :title="modLabel.titleFor(owner.modId)"
      >
        {{ modLabel.label(owner.modId) }}
        <span
          v-if="owner.modId === preview.base"
          class="text-text-faint"
        >{{ t("merge.header.baseTag") }}</span>
        <span
          v-if="owner.modId === preview.winner"
          class="text-text-faint"
        >{{ t("merge.header.winnerTag") }}</span>
      </span>
    </div>

    <div
      class="text-text-muted flex flex-wrap gap-4 text-xs"
      data-testid="merge-header-totals"
    >
      <span>{{ t("merge.header.totalsFields", { count: preview.totals.fields }, preview.totals.fields) }}</span>
      <span>{{ t("merge.header.totalsConflicts", { count: preview.totals.conflicts }, preview.totals.conflicts) }}</span>
      <span>{{ t("merge.header.totalsAuto", { count: preview.totals.auto }, preview.totals.auto) }}</span>
      <span>{{ t("merge.header.totalsUnresolved", { count: preview.totals.unresolved }, preview.totals.unresolved) }}</span>
      <span>{{ t("merge.header.totalsUnchanged", { count: preview.totals.unchanged }, preview.totals.unchanged) }}</span>
    </div>

    <p
      v-if="outOfScopeLine"
      class="text-status-input text-xs"
      data-testid="merge-header-out-of-scope"
    >
      {{ outOfScopeLine }}
    </p>

    <p
      v-if="structuralGuardLine"
      class="text-status-input text-xs font-medium"
      data-testid="merge-header-structural-guard"
    >
      {{ structuralGuardLine }}
    </p>

    <ul
      v-if="preview.caveats.length > 0"
      class="text-status-input flex flex-col gap-0.5 text-xs"
      data-testid="merge-header-caveats"
    >
      <li
        v-for="(caveat, index) in preview.caveats"
        :key="index"
      >
        {{ caveatLabel(caveat, t, modLabel.label, locale) }}
      </li>
    </ul>
  </div>
</template>
