<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { SourceDto } from "@/types/generated/SourceDto";
import { sourceBadgeLabel } from "@/utils/source";

/**
 * A mod's origin as one neutral icon plus visually hidden text and a
 * tooltip, both from `sourceBadgeLabel()` — never icon shape alone. `null` (a
 * missing mod, where discovery found no folder) is its own explicit
 * "origin unknown" state; the source is never inferred from a name or id.
 */
const { source } = defineProps<{ source: SourceDto | null }>();

const { t } = useI18n();
const tm = useTranslateMessage();

const ICON_BY_SOURCE: Readonly<Record<SourceDto, string>> = {
  core: "pi-box",
  dlc: "pi-th-large",
  local: "pi-folder",
  workshop: "pi-cloud",
};
const UNKNOWN_ICON = "pi-question-circle";

const label = computed(() =>
  source === null ? t("mods.table.sourceUnknown") : tm(sourceBadgeLabel(source)),
);
const icon = computed(() => (source === null ? UNKNOWN_ICON : ICON_BY_SOURCE[source]));
</script>

<template>
  <span
    role="img"
    class="text-text-muted inline-flex w-5 shrink-0 items-center justify-center gap-1.5 text-sm @lg:w-36 @lg:justify-start"
    :aria-label="label"
    :title="label"
    :data-source="source ?? 'unknown'"
    data-testid="source-badge"
  >
    <!-- One accessible name and tooltip on this wrapper. Not focusable: the row owns the roving tabindex. -->
    <i
      :class="['pi', icon]"
      aria-hidden="true"
    />
    <!-- Visible once the list itself is wide enough (a container query on the table: two lists share the window, so the window width says nothing) so a keyboard user, with no hover tooltip, sees the label too; aria-hidden as the wrapper already names it. -->
    <span
      class="hidden truncate text-xs @lg:inline"
      aria-hidden="true"
      data-testid="source-badge-label"
    >{{ label }}</span>
  </span>
</template>
