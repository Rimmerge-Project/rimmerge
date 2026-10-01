<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { EdgeDto } from "@/types/generated/EdgeDto";
import { edgeKindLabel } from "@/utils/edgeKind";
import { layerLabel } from "@/utils/layer";
import { edgeProvenanceDetail } from "@/utils/provenance";

/**
 * One accepted/dropped/advisory edge, shown from `modId`'s perspective —
 * the chip names the *other* mod and links to it in the load-order view,
 * plus to the inbox filtered to findings whose canonical key mentions
 * both mods.
 *
 * `FindingKey`'s exact canonical text (the `key=` a finding link uses)
 * is generated on the Rust side and never
 * exposed as a reusable format string to the frontend — reconstructing
 * it here would duplicate an internal `Display` impl the backend is free
 * to change. `list_findings`'s `search` filter is documented as "the
 * canonical key text contains this substring", and every edge-shaped
 * finding kind's key embeds `after` then `before` in that order, so a
 * `search=after:before` link reaches the same findings without that
 * duplication — it just shows an empty list rather than a dead link when
 * none exist.
 */
const { edge, modId } = defineProps<{
  edge: EdgeDto;
  /** The mod this chip is shown from the perspective of. */
  modId: string;
}>();

const otherModId = computed(() => (edge.after === modId ? edge.before : edge.after));
const findingSearch = computed(() => `${edge.after}:${edge.before}`);
const modLabel = useModLabel();
const tm = useTranslateMessage();
const { t } = useI18n();
const direction = computed(() =>
  edge.after === modId ? t("order.edgeChip.directionAfter") : t("order.edgeChip.directionBefore"),
);
const provenanceDetail = computed(() => edgeProvenanceDetail(edge, t));
</script>

<template>
  <span
    class="border-border-subtle bg-surface-1 inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs"
    :data-testid="`edge-chip-${otherModId}`"
  >
    <span class="text-text-faint">{{ direction }}</span>
    <RouterLink
      :to="`/order/${encodeURIComponent(otherModId)}`"
      class="text-accent font-medium underline"
      :data-testid="`edge-chip-mod-link-${otherModId}`"
      :title="modLabel.titleFor(otherModId)"
    >
      {{ modLabel.label(otherModId) }}
    </RouterLink>
    <span
      class="text-text-faint"
      :title="provenanceDetail"
    >{{ tm(layerLabel(edge.layer)) }}<template v-if="edge.kind"> · {{ tm(edgeKindLabel(edge.kind)) }}</template></span>
    <RouterLink
      :to="{ path: '/inbox', query: { search: findingSearch } }"
      class="text-accent underline"
      :data-testid="`edge-chip-finding-link-${otherModId}`"
    >
      {{ t("order.edgeChip.findingLink") }}
    </RouterLink>
  </span>
</template>
