<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import { computed, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import TierBadge from "@/components/order/TierBadge.vue";
import { useModLabel } from "@/composables/useModLabel";
import type { OrderRowDto } from "@/types/generated/OrderRowDto";

const { rows } = defineProps<{ rows: OrderRowDto[] }>();
const emit = defineEmits<{ selectRow: [modId: string] }>();
const { t } = useI18n();
const modLabel = useModLabel();

const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");

const ROW_HEIGHT_PX = 44;

// The whole options object is one `computed`, not individual reactive
// fields inside a plain object: `VirtualizerOptions.count` is typed as a
// plain `number`, and `useVirtualizer` only re-reads options when the
// object it was given itself changes (a plain object's own identity
// never does, so a bare `count: rows.length` would never update).
const virtualizer = useVirtualizer(
  computed(() => ({
    count: rows.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 12,
  })),
);

// `noUncheckedIndexedAccess` makes `rows[virtualRow.index]` an
// `OrderRowDto | undefined` — resolved once here (filtering out any
// index the virtualizer hands back past `rows.length`, which shouldn't
// happen but would otherwise crash the template) so the template itself
// only ever sees a real row.
interface VisibleRow {
  virtualRow: VirtualItem;
  row: OrderRowDto;
}

const visibleRows = computed<VisibleRow[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map((virtualRow) => ({ virtualRow, row: rows[virtualRow.index] }))
    .filter((entry): entry is VisibleRow => entry.row !== undefined),
);
const totalSize = computed(() => virtualizer.value.getTotalSize());
</script>

<template>
  <div
    class="surface-card flex min-h-0 flex-1 flex-col overflow-hidden"
    role="region"
    :aria-label="t('order.table.regionLabel')"
  >
    <div class="table-head bg-surface-1 flex shrink-0 items-center gap-3 px-3 py-2 font-semibold">
      <span class="w-12 text-right">#</span>
      <span class="w-16" />
      <span class="flex-1">{{ t("order.table.nameHeader") }}</span>
      <span class="w-16">{{ t("order.table.tierHeader") }}</span>
      <span class="w-10 text-right">{{ t("order.table.depsHeader") }}</span>
      <span class="w-8" />
    </div>

    <div
      ref="scrollElement"
      class="min-h-0 flex-1 overflow-y-auto"
      role="list"
      data-testid="order-table"
    >
      <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
        <div
          v-for="{ virtualRow, row } in visibleRows"
          :key="row.modId"
          role="listitem"
          tabindex="0"
          class="border-border-subtle hover:bg-surface-2 focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-3 border-b px-3 text-sm focus-visible:outline"
          :style="{
            height: `${virtualRow.size}px`,
            transform: `translateY(${virtualRow.start}px)`,
          }"
          :data-testid="`order-row-${row.modId}`"
          @click="emit('selectRow', row.modId)"
          @keydown.enter="emit('selectRow', row.modId)"
        >
          <span class="text-text-muted w-12 text-right tabular-nums">{{ row.position + 1 }}</span>
          <span
            v-if="row.previousPosition !== null && row.previousPosition !== row.position"
            class="text-text-faint w-16 text-xs"
            :data-testid="`order-row-moved-${row.modId}`"
          >
            {{ t("order.table.movedFrom", { position: row.previousPosition + 1 }) }}
          </span>
          <span
            v-else
            class="w-16"
          />
          <span
            class="text-text flex-1 truncate font-medium"
            :title="modLabel.titleFor(row.modId)"
          >{{ modLabel.label(row.modId) }}</span>
          <TierBadge :tier="row.tier" />
          <span class="text-text-muted w-10 text-right text-xs">{{ row.hardDependents }}</span>
          <span
            v-if="row.needsInputCount > 0"
            class="bg-status-input-soft text-status-input w-8 rounded-full px-1.5 py-0.5 text-center text-xs font-medium"
            :data-testid="`order-row-needs-input-${row.modId}`"
          >
            {{ row.needsInputCount }}
          </span>
          <span
            v-else
            class="w-8"
          />
        </div>
      </div>
    </div>
  </div>
</template>
