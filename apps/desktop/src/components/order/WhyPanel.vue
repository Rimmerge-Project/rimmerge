<script setup lang="ts">
import Dialog from "primevue/dialog";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import BaseEmpty from "@/components/base/BaseEmpty.vue";
import EdgeChip from "@/components/order/EdgeChip.vue";
import TierBadge from "@/components/order/TierBadge.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useDashboardQuery } from "@/queries/dashboard";
import { usePlacementQuery } from "@/queries/order";
import type { EdgeDto } from "@/types/generated/EdgeDto";
import type { LayerDto } from "@/types/generated/LayerDto";
import { layerLabel } from "@/utils/layer";
import { tierReasonText } from "@/utils/provenance";
import { describeSortProvenance } from "@/utils/sortProvenance";

/** `null` closes the panel; a mod id opens it and loads that mod's explanation. */
const { modId } = defineProps<{ modId: string | null }>();
const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
const tm = useTranslateMessage();
const router = useRouter();
const { data, isPending, error } = usePlacementQuery(() => modId);
// Cached alongside every other consumer of `get_dashboard` (Pinia Colada
// dedupes by key) — the why-panel doesn't need its own command for the
// sort-evidence settings' sort provenance, just the same
// data the dashboard tile and the apply dialog already fetch.
const { data: dashboard } = useDashboardQuery();
const modLabel = useModLabel();

interface EdgeGroup {
  layer: LayerDto;
  edges: EdgeDto[];
}

/** Groups consecutive same-layer edges — `lowerBounds`/`upperBounds` arrive `Hard`-first, already sorted by layer. */
function groupByLayer(edges: EdgeDto[]): EdgeGroup[] {
  const groups: EdgeGroup[] = [];
  for (const edge of edges) {
    const last = groups.at(-1);
    if (last && last.layer === edge.layer) {
      last.edges.push(edge);
    } else {
      groups.push({ layer: edge.layer, edges: [edge] });
    }
  }
  return groups;
}

const lowerBoundGroups = computed(() => groupByLayer(data.value?.lowerBounds ?? []));
const upperBoundGroups = computed(() => groupByLayer(data.value?.upperBounds ?? []));
const softAdvisory = computed(
  () => data.value?.advisory.filter((advisory) => advisory.strength === "soft") ?? [],
);
const awarenessAdvisory = computed(
  () => data.value?.advisory.filter((advisory) => advisory.strength === "awareness") ?? [],
);

function onVisibleChange(next: boolean): void {
  if (!next) {
    emit("close");
  }
}

/** Navigates to the mod info panel and closes this dialog — the two never show at once. */
function openInMods(): void {
  if (!data.value) {
    return;
  }
  void router.push({ name: "mod-detail", params: { modId: data.value.modId } });
  emit("close");
}
</script>

<template>
  <Dialog
    :visible="modId !== null"
    modal
    class="w-[48rem]"
    data-testid="why-panel"
    @update:visible="onVisibleChange"
  >
    <template #header>
      <div class="flex items-center gap-2">
        <span class="p-dialog-title">{{
          data
            ? t("order.whyPanel.headerWithMod", { mod: modLabel.label(data.modId) })
            : t("order.whyPanel.headerFallback")
        }}</span>
        <button
          v-if="data"
          type="button"
          class="text-accent cursor-pointer text-xs underline"
          data-testid="why-panel-open-in-mods"
          @click="openInMods"
        >
          {{ t("order.whyPanel.openInModsLink") }}
        </button>
      </div>
    </template>

    <p
      v-if="isPending"
      data-testid="why-panel-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger"
      data-testid="why-panel-error"
    >
      {{ error instanceof Error ? error.message : t("order.whyPanel.loadFailed") }}
    </p>

    <div
      v-else-if="data"
      class="flex flex-col gap-4"
      data-testid="why-panel-content"
    >
      <section>
        <h3 class="font-semibold">
          {{ t("order.whyPanel.positionHeading") }}
        </h3>
        <p class="flex items-center gap-2">
          <span>#{{ data.position + 1 }}</span>
          <TierBadge :tier="data.tier" />
          <span class="text-text-muted text-sm">{{
            tierReasonText(data.tierReason, t, modLabel.label)
          }}</span>
        </p>
      </section>

      <section
        v-if="data.becameReadyAfter"
        data-testid="why-panel-became-ready-after"
      >
        <h3 class="font-semibold">
          {{ t("order.whyPanel.becameReadyAfterHeading") }}
        </h3>
        <EdgeChip
          :edge="data.becameReadyAfter"
          :mod-id="data.modId"
        />
      </section>

      <section data-testid="why-panel-lower-bounds">
        <h3 class="font-semibold">
          {{ t("order.whyPanel.lowerBoundsHeading") }}
        </h3>
        <BaseEmpty
          v-if="lowerBoundGroups.length === 0"
          :message="t('order.whyPanel.noLowerBounds')"
        />
        <div
          v-for="group in lowerBoundGroups"
          :key="group.layer"
          class="mb-2"
        >
          <div class="text-xs text-text-faint tracking-wide uppercase">
            {{ tm(layerLabel(group.layer)) }}
          </div>
          <div class="flex flex-wrap gap-1">
            <EdgeChip
              v-for="edge in group.edges"
              :key="`${edge.after}|${edge.before}`"
              :edge="edge"
              :mod-id="data.modId"
            />
          </div>
        </div>
      </section>

      <section data-testid="why-panel-upper-bounds">
        <h3 class="font-semibold">
          {{ t("order.whyPanel.upperBoundsHeading") }}
        </h3>
        <BaseEmpty
          v-if="upperBoundGroups.length === 0"
          :message="t('order.whyPanel.noUpperBounds')"
        />
        <div
          v-for="group in upperBoundGroups"
          :key="group.layer"
          class="mb-2"
        >
          <div class="text-xs text-text-faint tracking-wide uppercase">
            {{ tm(layerLabel(group.layer)) }}
          </div>
          <div class="flex flex-wrap gap-1">
            <EdgeChip
              v-for="edge in group.edges"
              :key="`${edge.after}|${edge.before}`"
              :edge="edge"
              :mod-id="data.modId"
            />
          </div>
        </div>
      </section>

      <section data-testid="why-panel-advisory">
        <h3 class="font-semibold">
          {{ t("order.whyPanel.advisoryHeading") }}
        </h3>
        <div>
          <div class="text-xs text-text-faint tracking-wide uppercase">
            {{ t("order.whyPanel.softHeading") }}
          </div>
          <BaseEmpty
            v-if="softAdvisory.length === 0"
            :message="t('order.whyPanel.none')"
          />
          <div class="flex flex-wrap gap-1">
            <span
              v-for="advisory in softAdvisory"
              :key="`${advisory.edge.after}|${advisory.edge.before}`"
              class="flex items-center gap-1"
            >
              <EdgeChip
                :edge="advisory.edge"
                :mod-id="data.modId"
              />
              <span
                :class="advisory.satisfied ? 'text-status-auto' : 'text-status-danger'"
                :data-testid="advisory.satisfied ? 'advisory-satisfied' : 'advisory-violated'"
              >
                {{ advisory.satisfied ? t("order.whyPanel.satisfied") : t("order.whyPanel.violated") }}
              </span>
            </span>
          </div>
        </div>
        <div class="mt-2">
          <div class="text-xs text-text-faint tracking-wide uppercase">
            {{ t("order.whyPanel.awarenessHeading") }}
          </div>
          <BaseEmpty
            v-if="awarenessAdvisory.length === 0"
            :message="t('order.whyPanel.none')"
          />
          <div class="flex flex-wrap gap-1">
            <span
              v-for="advisory in awarenessAdvisory"
              :key="`${advisory.edge.after}|${advisory.edge.before}`"
              class="flex items-center gap-1"
            >
              <EdgeChip
                :edge="advisory.edge"
                :mod-id="data.modId"
              />
              <span
                :class="advisory.satisfied ? 'text-status-auto' : 'text-status-danger'"
                :data-testid="advisory.satisfied ? 'advisory-satisfied' : 'advisory-violated'"
              >
                {{ advisory.satisfied ? t("order.whyPanel.satisfied") : t("order.whyPanel.violated") }}
              </span>
            </span>
          </div>
        </div>
      </section>

      <section
        v-if="data.dropped.length > 0"
        data-testid="why-panel-dropped"
      >
        <h3 class="font-semibold">
          {{ t("order.whyPanel.droppedHeading") }}
        </h3>
        <div
          v-for="dropped in data.dropped"
          :key="`${dropped.edge.after}|${dropped.edge.before}`"
          class="mb-1 flex flex-col gap-1"
        >
          <div class="flex items-center gap-2">
            <EdgeChip
              :edge="dropped.edge"
              :mod-id="data.modId"
            />
            <span class="text-text-faint text-xs">{{
              t("order.whyPanel.cycle", { cycle: dropped.witnessCycle.join(" → ") })
            }}</span>
          </div>
          <div
            v-if="dropped.winner"
            class="flex items-center gap-2 pl-1"
            data-testid="why-panel-dropped-winner"
          >
            <span class="text-text-muted text-xs">{{ t("order.whyPanel.overruledBy") }}</span>
            <EdgeChip
              :edge="dropped.winner"
              :mod-id="data.modId"
            />
          </div>
        </div>
      </section>

      <section
        v-if="dashboard"
        data-testid="why-panel-sort-settings"
      >
        <h3 class="font-semibold">
          {{ t("order.whyPanel.sortSettingsHeading") }}
        </h3>
        <p class="text-text-muted text-sm">
          {{ describeSortProvenance(dashboard.sortProvenance, t) }}
        </p>
      </section>

      <section data-testid="why-panel-tie-break">
        <h3 class="font-semibold">
          {{ t("order.whyPanel.tieBreakHeading") }}
        </h3>
        <ul class="flex flex-col gap-1 text-sm">
          <li>
            {{
              data.tieBreak.currentPosition !== null
                ? t("order.whyPanel.currentPositionKnown", {
                  position: data.tieBreak.currentPosition + 1,
                })
                : t("order.whyPanel.currentPositionUnknown")
            }}
          </li>
          <li>{{ t("order.whyPanel.effectiveKey", { key: data.tieBreak.effectiveKey }) }}</li>
          <li>
            {{
              t("order.whyPanel.modsPreferredAhead", { count: data.tieBreak.modsPreferredAhead })
            }}
          </li>
          <li
            v-if="data.tieBreak.pulledForwardBy"
            class="flex items-center gap-2"
          >
            {{ t("order.whyPanel.pulledForwardBy") }}
            <EdgeChip
              :edge="data.tieBreak.pulledForwardBy"
              :mod-id="data.modId"
            />
          </li>
        </ul>
      </section>
    </div>
  </Dialog>
</template>
