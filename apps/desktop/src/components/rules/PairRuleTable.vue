<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import Button from "primevue/button";
import { computed, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { DeleteRuleRequestDto } from "@/types/generated/DeleteRuleRequestDto";
import type { PairRuleDto } from "@/types/generated/PairRuleDto";
import type { RuleKeyDto } from "@/types/generated/RuleKeyDto";
import { isImportedOrigin, ruleOriginLabel } from "@/utils/rule";

/**
 * Virtualized on purpose: a RimSort/SteamDB import yields several hundred
 * pair rules, and mounting one PrimeVue `Button` per row was the rules
 * page's dominant cost (~250 ms per render of 800 rows, repeated on every
 * tab switch). Same pattern as `ModTable.vue`/`OrderTable.vue`.
 */
const { pairs, useImportedPairs } = defineProps<{
  pairs: PairRuleDto[];
  /** Whether imported pair rules currently feed the sorter — greys imported rows when `false`. */
  useImportedPairs: boolean;
}>();
const emit = defineEmits<{ delete: [request: DeleteRuleRequestDto]; promote: [key: RuleKeyDto] }>();
const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");
const ROW_HEIGHT_PX = 36;

// The whole options object is one `computed` — see `OrderTable.vue`'s
// identical comment for why.
const virtualizer = useVirtualizer(
  computed(() => ({
    count: pairs.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 12,
  })),
);

interface VisibleRow {
  virtualRow: VirtualItem;
  pair: PairRuleDto;
}

const visibleRows = computed<VisibleRow[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map((virtualRow) => ({ virtualRow, pair: pairs[virtualRow.index] }))
    .filter((entry): entry is VisibleRow => entry.pair !== undefined),
);
const totalSize = computed(() => virtualizer.value.getTotalSize());

/** Greyed exactly when this is an imported row and the pairs toggle is off. */
function isGreyedOut(pair: PairRuleDto): boolean {
  return isImportedOrigin(pair.origin) && !useImportedPairs;
}

/** The origin cell's tooltip: the whole text the truncated cell may hide, including its tags. */
function originTitle(pair: PairRuleDto): string {
  const parts = [tm(ruleOriginLabel(pair.origin))];
  if (pair.promotedFrom) {
    parts.push(
      t("rules.pairTable.promotedFrom", { origin: tm(ruleOriginLabel(pair.promotedFrom)) }),
    );
  }
  if (pair.overridesDeclared) {
    parts.push(t("rules.pairTable.overridesDeclaredTag"));
  }
  return parts.join(" ");
}

/**
 * A promoted copy shares its imported original's `(after, before)` — both
 * are listed at once ("promote to user rule") — so `origin` must be
 * part of the row's identity, not just the pair, or two simultaneous rows
 * would collide on the same `:key`/`data-testid`.
 */
function pairRowId(pair: PairRuleDto): string {
  return `${pair.after}-${pair.before}-${pair.origin}`;
}
</script>

<template>
  <div
    ref="scrollElement"
    class="surface-card max-h-[32rem] overflow-y-auto"
    role="table"
    :aria-label="t('rules.pairTable.regionLabel')"
    data-testid="pair-rule-table"
  >
    <div
      role="row"
      class="table-head grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_10rem_minmax(0,1fr)_9rem_6rem] items-center gap-2 px-2 py-1 text-left text-sm font-medium"
    >
      <span
        role="columnheader"
        class="truncate"
      >{{ t("rules.pairTable.afterHeader") }}</span>
      <span
        role="columnheader"
        class="truncate"
      >{{ t("rules.pairTable.beforeHeader") }}</span>
      <span
        role="columnheader"
        class="truncate"
      >{{ t("rules.pairTable.originHeader") }}</span>
      <span
        role="columnheader"
        class="truncate"
      >{{ t("rules.pairTable.commentHeader") }}</span>
      <span role="columnheader" />
      <span role="columnheader" />
    </div>
    <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
      <div
        v-for="{ virtualRow, pair } in visibleRows"
        :key="pairRowId(pair)"
        role="row"
        class="border-border-subtle absolute top-0 left-0 grid w-full grid-cols-[minmax(0,1fr)_minmax(0,1fr)_10rem_minmax(0,1fr)_9rem_6rem] items-center gap-2 border-b px-2 text-sm"
        :title="isGreyedOut(pair) ? t('rules.pairTable.notAppliedTitle') : undefined"
        :style="{
          height: `${virtualRow.size}px`,
          transform: `translateY(${virtualRow.start}px)`,
        }"
        :data-testid="`pair-rule-${pairRowId(pair)}`"
      >
        <span
          v-if="isGreyedOut(pair)"
          class="sr-only"
        >{{ t("rules.pairTable.notAppliedTitle") }}</span>
        <span
          role="cell"
          class="truncate"
          :class="{ 'opacity-50': isGreyedOut(pair) }"
          :title="modLabel.titleFor(pair.after)"
        >{{ modLabel.label(pair.after) }}</span>
        <span
          role="cell"
          class="truncate"
          :class="{ 'opacity-50': isGreyedOut(pair) }"
          :title="modLabel.titleFor(pair.before)"
        >{{ modLabel.label(pair.before) }}</span>
        <span
          role="cell"
          class="truncate"
          :class="{ 'opacity-50': isGreyedOut(pair) }"
          :title="originTitle(pair)"
        >
          {{ tm(ruleOriginLabel(pair.origin)) }}
          <span
            v-if="pair.promotedFrom"
            class="text-text-faint text-xs"
            :data-testid="`pair-rule-${pairRowId(pair)}-promoted-from`"
          >
            {{ t("rules.pairTable.promotedFrom", { origin: tm(ruleOriginLabel(pair.promotedFrom)) }) }}
          </span>
          <span
            v-if="pair.overridesDeclared"
            class="text-status-override text-xs font-medium"
            :title="t('rules.pairTable.overridesDeclaredTitle')"
            :data-testid="`pair-rule-${pairRowId(pair)}-overrides-declared`"
          >
            {{ t("rules.pairTable.overridesDeclaredTag") }}
          </span>
        </span>
        <span
          role="cell"
          class="truncate"
          :class="{ 'opacity-50': isGreyedOut(pair) }"
          :title="pair.comment ?? undefined"
        >{{ pair.comment }}</span>
        <span role="cell">
          <Button
            v-if="isImportedOrigin(pair.origin)"
            :label="
              pair.alreadyPromoted
                ? t('rules.pairTable.promotedButton')
                : t('rules.pairTable.promoteButton')
            "
            size="small"
            class="whitespace-nowrap"
            text
            :disabled="pair.alreadyPromoted"
            :data-testid="`pair-rule-${pairRowId(pair)}-promote`"
            @click="emit('promote', { kind: 'pair', after: pair.after, before: pair.before })"
          />
        </span>
        <span role="cell">
          <Button
            :label="t('rules.pairTable.deleteButton')"
            size="small"
            class="whitespace-nowrap"
            severity="danger"
            text
            @click="
              emit('delete', {
                key: { kind: 'pair', after: pair.after, before: pair.before },
                origin: pair.origin,
              })
            "
          />
        </span>
      </div>
    </div>
  </div>
</template>
