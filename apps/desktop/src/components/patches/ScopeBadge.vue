<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { formatList } from "@/i18n/format";

/**
 * The "partial: +c, +d" chip a finding card shows when a compat patch's
 * scope only partly covers the finding's owners. Only ever rendered for
 * `ScopeMembershipDto`'s `"partial"` arm — a `"full"` membership shows
 * nothing, and `FindingCard.vue` is the one that decides that (`v-if`),
 * not this component, so it stays a plain presentational leaf.
 */
const { outside } = defineProps<{
  /** The scope members' finding co-owners that aren't in the patch's scope. */
  outside: string[];
}>();

const { t, locale } = useI18n();
const modLabel = useModLabel();
const names = computed(() => outside.map((id) => modLabel.label(id)));
// The tooltip is a real sentence — locale-joined. `label` is a compact
// "+a, +b" chip, not a natural-language list (each name keeps its own
// leading "+"), so it keeps its own literal separator on purpose.
const title = computed(() =>
  t("patches.scopeBadge.title", { mods: formatList(locale.value, names.value) }),
);
const label = computed(() => t("patches.scopeBadge.label", { mods: names.value.join(", +") }));
</script>

<template>
  <span
    class="bg-status-input-soft text-status-input shrink-0 rounded-full px-2 py-0.5 text-xs font-medium whitespace-nowrap"
    :title="title"
    data-testid="scope-badge"
  >
    {{ label }}
  </span>
</template>
