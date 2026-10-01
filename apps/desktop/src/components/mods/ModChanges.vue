<script setup lang="ts">
import { refDebounced } from "@vueuse/core";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { usePagedRows } from "@/composables/usePagedRows";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useModChangesQuery } from "@/queries/defs";
import type { ChangeFilterDto } from "@/types/generated/ChangeFilterDto";
import type { ChangeKindDto } from "@/types/generated/ChangeKindDto";
import type { ChangeRowDto } from "@/types/generated/ChangeRowDto";
import { CHANGE_KIND_ORDER, changeKindLabel } from "@/utils/changeKind";
import { describeFindingKey } from "@/utils/finding";
import { inspectRoute } from "@/utils/format";

const { modId } = defineProps<{ modId: string }>();
const { t } = useI18n();
const tm = useTranslateMessage();

/** Matches `rim_session::MAX_PAGE_SIZE` — the server caps `filter.limit` to this. */
const PAGE_SIZE = 200;

const selectedKind = ref<ChangeKindDto | null>(null);
const searchInput = ref("");
const search = refDebounced(searchInput, 200);
const offset = ref(0);

const filter = computed<ChangeFilterDto>(() => ({
  kinds: selectedKind.value === null ? null : [selectedKind.value],
  search: search.value.length > 0 ? search.value : null,
  offset: offset.value,
  limit: PAGE_SIZE,
}));

const query = useModChangesQuery(() => modId, filter);

const paged = usePagedRows(
  offset,
  computed(() =>
    query.data.value ? { items: query.data.value.items, total: query.data.value.total } : null,
  ),
  () => [selectedKind.value, search.value],
);

const kindCounts = computed(() => query.data.value?.kindCounts ?? null);

function toggleKind(kind: ChangeKindDto): void {
  selectedKind.value = selectedKind.value === kind ? null : kind;
}

function targetLabel(row: ChangeRowDto): string {
  return row.defRef ?? row.assetPath ?? "";
}
</script>

<template>
  <div
    class="flex flex-col gap-3"
    data-testid="mod-changes"
  >
    <div class="flex flex-wrap items-center gap-2">
      <button
        v-for="kind in CHANGE_KIND_ORDER"
        :key="kind"
        type="button"
        class="cursor-pointer rounded-full border px-2 py-0.5 text-xs"
        :class="
          selectedKind === kind
            ? 'border-accent text-accent bg-accent/10'
            : 'border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text'
        "
        :data-testid="`mod-changes-chip-${kind}`"
        @click="toggleKind(kind)"
      >
        {{ tm(changeKindLabel(kind)) }} ({{ kindCounts?.[kind] ?? 0 }})
      </button>
      <input
        v-model="searchInput"
        type="search"
        :placeholder="t('mods.changes.searchPlaceholder')"
        class="border-border-subtle bg-surface-1 ml-auto rounded border px-2 py-1 text-xs"
        data-testid="mod-changes-search"
      >
    </div>

    <p
      v-if="query.error.value"
      class="text-status-danger text-sm"
      data-testid="mod-changes-error"
    >
      {{
        query.error.value instanceof Error
          ? query.error.value.message
          : t("mods.changes.loadFailed")
      }}
    </p>
    <p
      v-else-if="query.isPending.value"
      class="text-text-muted text-sm"
      data-testid="mod-changes-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="paged.rows.value.length === 0"
      class="text-text-muted text-sm"
      data-testid="mod-changes-empty"
    >
      {{ t("mods.changes.noMatches") }}
    </p>

    <table
      v-else
      class="w-full text-left text-sm"
      data-testid="mod-changes-table"
    >
      <thead>
        <tr class="text-text-faint text-xs uppercase">
          <th class="py-1 font-medium">
            {{ t("mods.changes.kindHeader") }}
          </th>
          <th class="py-1 font-medium">
            {{ t("mods.changes.targetHeader") }}
          </th>
          <th class="py-1 font-medium">
            {{ t("mods.changes.opsHeader") }}
          </th>
          <th class="py-1 font-medium">
            {{ t("mods.changes.otherTouchersHeader") }}
          </th>
          <th class="py-1 font-medium">
            {{ t("mods.changes.findingsHeader") }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="row in paged.rows.value"
          :key="`${row.kind}|${targetLabel(row)}`"
          class="border-border-subtle border-t"
          :data-testid="`mod-changes-row-${targetLabel(row)}`"
        >
          <td class="py-1">
            {{ tm(changeKindLabel(row.kind)) }}
          </td>
          <td class="py-1">
            <RouterLink
              v-if="row.defRef"
              :to="inspectRoute(row.defRef)"
              class="text-accent underline"
            >
              {{ row.defRef }}
            </RouterLink>
            <span v-else>{{ row.assetPath }}</span>
          </td>
          <td class="py-1">
            {{ row.opCount }}
          </td>
          <td class="py-1">
            {{ row.otherTouchers }}
          </td>
          <td class="py-1">
            <ul
              v-if="row.findingKeys.length > 0"
              class="flex flex-wrap gap-1"
            >
              <li
                v-for="key in row.findingKeys"
                :key="key"
              >
                <RouterLink
                  :to="{ path: '/inbox', query: { search: key } }"
                  :title="key"
                  class="bg-surface-2 text-text-muted rounded px-1.5 py-0.5 text-xs"
                >
                  {{ tm(describeFindingKey(key)) }}
                </RouterLink>
              </li>
            </ul>
          </td>
        </tr>
      </tbody>
    </table>

    <button
      v-if="paged.hasMore.value"
      type="button"
      class="border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text cursor-pointer self-start rounded border px-2 py-1 text-xs"
      data-testid="mod-changes-show-more"
      @click="paged.loadMore"
    >
      {{
        t("mods.changes.showMore", { loaded: paged.rows.value.length, total: paged.total.value })
      }}
    </button>
  </div>
</template>
