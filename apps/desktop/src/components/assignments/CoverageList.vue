<script setup lang="ts">
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import DefThumbnail from "@/components/graphics/DefThumbnail.vue";
import { useModLabel } from "@/composables/useModLabel";
import { defRefOf } from "@/types/brands";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";

/**
 * The editor's left pane: the coverage work queue, uncovered-first (the
 * backend's own order), filtered client-side by target-mod and search.
 * No key-type filter: the current
 * schemas only ever have one `TargetKey` field's worth of
 * candidates in practice (Example's own `speciesNames`), so a second filter
 * dimension had no real data to exercise; documented here rather than
 * silently narrowed.
 */
const { rows, selectedTarget = null } = defineProps<{
  rows: CoverageRowDto[];
  selectedTarget?: CoverageRowDto["target"] | null;
}>();
const emit = defineEmits<{ select: [row: CoverageRowDto] }>();

const { t } = useI18n();
const modLabel = useModLabel();

const search = ref("");
const ownerFilter = ref<string | null>(null);

const ownerOptions = computed(() => {
  const owners = new Set(rows.map((row) => row.owner));
  return [...owners].sort().map((owner) => ({ label: modLabel.label(owner), value: owner }));
});

const filtered = computed(() =>
  rows.filter((row) => {
    if (ownerFilter.value && row.owner !== ownerFilter.value) {
      return false;
    }
    if (search.value.length === 0) {
      return true;
    }
    return row.target.def.defName.toLowerCase().includes(search.value.toLowerCase());
  }),
);

function isSelected(row: CoverageRowDto): boolean {
  return (
    selectedTarget?.def.defType === row.target.def.defType &&
    selectedTarget?.def.defName === row.target.def.defName
  );
}
</script>

<template>
  <div
    class="flex flex-col gap-2"
    data-testid="coverage-list"
  >
    <InputText
      v-model="search"
      :placeholder="t('assignments.coverageList.searchPlaceholder')"
      size="small"
      data-testid="coverage-search"
    />
    <Select
      v-model="ownerFilter"
      :options="ownerOptions"
      option-label="label"
      option-value="value"
      show-clear
      :placeholder="t('assignments.coverageList.ownerFilterPlaceholder')"
      size="small"
      data-testid="coverage-owner-filter"
    />
    <ul class="flex flex-col gap-0.5 overflow-y-auto">
      <li
        v-for="row in filtered"
        :key="`${row.target.def.defType}/${row.target.def.defName}`"
      >
        <button
          type="button"
          class="hover:bg-surface-2 flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-left text-xs"
          :class="{ 'bg-accent-soft': isSelected(row) }"
          :aria-current="isSelected(row)"
          :data-testid="`coverage-row-${row.target.def.defName}`"
          @click="emit('select', row)"
        >
          <DefThumbnail
            class="h-10 w-10 shrink-0"
            :def-ref="defRefOf(row.target.def.defType, row.target.def.defName)"
          />
          <span class="flex min-w-0 flex-col gap-0.5">
            <span class="font-medium">{{ row.target.def.defName }}</span>
            <span class="text-text-faint flex items-center gap-1">
              <span
                class="rounded-full px-1.5"
                :class="
                  row.intent === 'cover' && !row.hasRow
                    ? 'bg-status-input-soft text-status-input'
                    : 'bg-surface-2'
                "
                :data-testid="`coverage-cell-${row.target.def.defName}`"
              >
                {{
                  row.hasRow
                    ? t("assignments.coverageList.thisProject")
                    : row.intent === "override"
                      ? row.matches[0]
                        ? t("assignments.coverageList.coveredByMod", {
                          mod: modLabel.label(row.matches[0].owner),
                        })
                        : t("assignments.coverageList.coveredByAnotherMod")
                      : t("assignments.coverageList.uncovered")
                }}
              </span>
              <span v-if="row.winner?.kind === 'thisProject'">{{
                t("assignments.coverageList.wouldWinNoOwner")
              }}</span>
              <span v-else-if="row.winner?.kind === 'existing'">
                {{
                  t("assignments.coverageList.wouldWinWithOwner", {
                    mod: modLabel.label(row.winner.match.owner),
                  })
                }}
              </span>
            </span>
          </span>
        </button>
      </li>
      <li
        v-if="filtered.length === 0"
        class="text-text-faint px-2 py-1 text-xs"
      >
        {{ t("assignments.coverageList.noMatches") }}
      </li>
    </ul>
  </div>
</template>
