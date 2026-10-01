<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { EdgeStrengthDto } from "@/types/generated/EdgeStrengthDto";
import type { ModEdgeDto } from "@/types/generated/ModEdgeDto";
import { edgeKindLabel } from "@/utils/edgeKind";
import { edgeStatusLabel } from "@/utils/edgeStatus";
import { edgeStrengthLabel } from "@/utils/edgeStrength";

const { edges } = defineProps<{ edges: ModEdgeDto[] }>();
const { t } = useI18n();
const modLabel = useModLabel();
const tm = useTranslateMessage();

// Display order for each edge strength (most authoritative first) — a
// `Record` over every `EdgeStrengthDto` variant, not a bare array
// literal: a plain `readonly EdgeStrengthDto[]` silently drops a
// strength missing from it (its own group just never renders, with
// nothing to say a case was missed), while `Record<EdgeStrengthDto,
// number>` forces the object literal below to carry every variant as a
// key — a strength added to the DTO with no entry here fails to compile
// instead.
const STRENGTH_RANK: Record<EdgeStrengthDto, number> = {
  hard: 0,
  declared: 1,
  soft: 2,
  inferred: 3,
  awareness: 4,
};
const STRENGTH_ORDER: readonly EdgeStrengthDto[] = (
  Object.keys(STRENGTH_RANK) as EdgeStrengthDto[]
).sort((a, b) => STRENGTH_RANK[a] - STRENGTH_RANK[b]);

const grouped = computed(() =>
  STRENGTH_ORDER.map((strength) => ({
    strength,
    edges: edges.filter((edge) => edge.strength === strength),
  })).filter((group) => group.edges.length > 0),
);
</script>

<template>
  <div
    class="flex flex-col gap-3"
    data-testid="mod-edges"
  >
    <p
      v-if="edges.length === 0"
      class="text-text-muted text-sm"
    >
      {{ t("mods.edges.none") }}
    </p>
    <div
      v-for="group in grouped"
      :key="group.strength"
    >
      <div class="text-text-faint text-xs tracking-wide uppercase">
        {{ tm(edgeStrengthLabel(group.strength)) }}
      </div>
      <ul class="flex flex-col gap-1">
        <li
          v-for="edge in group.edges"
          :key="`${edge.otherModId}|${edge.kind}`"
          class="border-border-subtle bg-surface-1 flex items-center justify-between gap-2 rounded border px-2 py-1 text-sm"
          :data-testid="`mod-edge-${edge.otherModId}`"
        >
          <RouterLink
            :to="`/mods/${encodeURIComponent(edge.otherModId)}`"
            class="text-accent underline"
            :title="modLabel.titleFor(edge.otherModId)"
          >
            {{ modLabel.label(edge.otherModId) }}
          </RouterLink>
          <span class="text-text-faint text-xs">{{ tm(edgeKindLabel(edge.kind)) }}</span>
          <span
            :class="{
              'text-status-auto': edge.status === 'satisfied',
              'text-status-danger': edge.status === 'violated',
              'text-text-faint': edge.status === 'unevaluated',
            }"
            :data-testid="`mod-edge-status-${edge.otherModId}`"
          >
            {{ tm(edgeStatusLabel(edge.status)) }}
          </span>
        </li>
      </ul>
    </div>
  </div>
</template>
