<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { type RouteLocationRaw, useRouter } from "vue-router";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import type { MergeModEntryDto } from "@/types/generated/MergeModEntryDto";
import { describeMergeState, mergeEntryKindLabel, mergeStateBadgeClasses } from "@/utils/format";

const {
  entries,
  // The profile merge editor route — every caller but `PatchExportPanel`
  // (which passes one that targets `patch-merge-editor` instead) relies
  // on this default. Inlined, not a named helper referenced here: a
  // `defineProps` destructure default is hoisted out of `setup()` and
  // can't reference another script-scope binding.
  editorRoute = (key: string) => ({ name: "merge-editor", params: { key } }),
} = defineProps<{
  entries: MergeModEntryDto[];
  /** Where "Open in editor" navigates for a given entry's key — defaults to the profile merge editor. `PatchExportPanel` passes one that targets `patch-merge-editor` instead. */
  editorRoute?: (key: string) => RouteLocationRaw;
}>();
const modLabel = useModLabel();
const { t, locale } = useI18n();
const tm = useTranslateMessage();

interface EntryGroup {
  defType: string;
  entries: MergeModEntryDto[];
}

/**
 * Grouped by def type (`entry.defKey?.defType`, {@link t}`("merge.mod.assetsGroup")` for
 * `ShipAsset` entries, which carry no `defKey`), in the order each group
 * is first seen — stable and readable without a second sort key.
 */
const groups = computed<EntryGroup[]>(() => {
  const byType = new Map<string, MergeModEntryDto[]>();
  for (const entry of entries) {
    const defType = entry.defKey?.defType ?? t("merge.mod.assetsGroup");
    const group = byType.get(defType);
    if (group) {
      group.push(entry);
    } else {
      byType.set(defType, [entry]);
    }
  }
  return [...byType.entries()].map(([defType, groupEntries]) => ({
    defType,
    entries: groupEntries,
  }));
});

const router = useRouter();

/** Only `defOverride`/`patchCollision` entries have a merge editor to open — an `asset` entry is a `ShipAsset` decision, not a `Merge` one. */
function isEditable(entry: MergeModEntryDto): boolean {
  return entry.kind !== "asset";
}

function openInEditor(entry: MergeModEntryDto): void {
  void router.push(editorRoute(entry.key));
}
</script>

<template>
  <div
    class="flex flex-col gap-4"
    data-testid="merge-mod-entry-list"
  >
    <section
      v-for="group in groups"
      :key="group.defType"
    >
      <h3 class="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
        {{ group.defType }}
      </h3>
      <ul class="surface-card divide-border-subtle divide-y">
        <li
          v-for="entry in group.entries"
          :key="entry.key"
          class="flex flex-col gap-1.5 p-3 text-sm"
          :data-testid="`merge-mod-entry-${entry.key}`"
        >
          <span class="text-text truncate font-medium">
            {{ entry.defKey ? `${entry.defKey.defType}/${entry.defKey.defName}` : entry.key }}
          </span>
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-text-muted shrink-0 text-xs">{{ tm(mergeEntryKindLabel(entry.kind)) }}</span>
            <span
              class="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium"
              :class="mergeStateBadgeClasses(entry.state)"
            >
              {{ tm(describeMergeState(entry.state, entry.structuralGuardField)) }}
            </span>
            <span class="text-text-muted shrink-0 text-xs">{{
              t("merge.mod.opsCount", { count: entry.opCount }, entry.opCount)
            }}</span>
            <span
              v-if="entry.dependsOn.length > 0"
              class="text-text-muted shrink-0 text-xs"
            >
              {{ t("merge.mod.dependsOn") }}
              {{ formatList(locale, entry.dependsOn.map((modId) => modLabel.label(modId))) }}
            </span>
            <button
              v-if="isEditable(entry)"
              type="button"
              class="text-accent ml-auto shrink-0 cursor-pointer text-xs underline focus-visible:outline"
              :data-testid="`merge-mod-open-editor-${entry.key}`"
              @click="openInEditor(entry)"
            >
              {{ t("merge.mod.openInEditor") }}
            </button>
          </div>
        </li>
      </ul>
    </section>
  </div>
</template>
