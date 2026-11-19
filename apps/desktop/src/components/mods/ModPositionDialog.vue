<script setup lang="ts">
import { refDebounced } from "@vueuse/core";
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useWorkingModsQuery } from "@/queries/activeSet";
import type { AnchorDto } from "@/types/generated/AnchorDto";

/**
 * The keyboard-and-screen-reader way to name a position: a search over the Active list, each
 * match meaning "right after this mod". It backs "Move after…" (`mode: "move"`) and an inactive
 * row's "Activate at…" (`mode: "activate"`, which also offers the two ends). Mods that are
 * moving are not offered, since a mod cannot be placed next to itself. The result is an anchor
 * naming a neighbour by id, the same thing a drop produces.
 */
const { mode, excludeIds } = defineProps<{
  mode: "move" | "activate";
  /** The mods being placed; they are left out of the matches. */
  excludeIds: readonly string[];
}>();
const emit = defineEmits<{ choose: [anchor: AnchorDto] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const modLabel = useModLabel();

/** The most matches drawn; the search narrows the rest. */
const MAX_MATCHES = 100;

const search = ref("");
const debouncedSearch = refDebounced(search, 300);
const { data } = useWorkingModsQuery(
  () => ({
    search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
    tag: null,
    source: null,
    sort: null,
  }),
  () => visible.value,
);

const excluded = computed(() => new Set(excludeIds));
const candidates = computed(() =>
  (data.value?.items ?? []).filter((row) => !excluded.value.has(row.modId)),
);
const matches = computed(() => candidates.value.slice(0, MAX_MATCHES));
const isCapped = computed(() => candidates.value.length > MAX_MATCHES);

// A fresh search each time the dialog opens.
watch(visible, (isOpen) => {
  if (isOpen) {
    search.value = "";
  }
});

function choose(anchor: AnchorDto): void {
  // Emitted before closing: closing clears the request the parent is about to act on.
  emit("choose", anchor);
  visible.value = false;
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="mode === 'move' ? t('mods.move.afterDialogHeader') : t('mods.move.activateAtDialogHeader')"
    :style="{ width: '28rem' }"
    data-testid="mod-position-dialog"
  >
    <div class="flex flex-col gap-3">
      <p class="text-text-muted text-sm">
        {{ t("mods.move.pickHelp") }}
      </p>
      <div
        v-if="mode === 'activate'"
        class="flex flex-wrap gap-2"
      >
        <Button
          :label="t('mods.move.atStart')"
          severity="secondary"
          outlined
          size="small"
          data-testid="mod-position-start"
          @click="choose({ kind: 'start' })"
        />
        <Button
          :label="t('mods.move.atEnd')"
          severity="secondary"
          outlined
          size="small"
          data-testid="mod-position-end"
          @click="choose({ kind: 'end' })"
        />
      </div>
      <InputText
        v-model="search"
        :placeholder="t('mods.picker.searchPlaceholder')"
        :aria-label="t('mods.page.searchActiveLabel')"
        data-testid="mod-position-search"
      />
      <ul
        class="border-border-subtle bg-surface-1 flex max-h-72 flex-col gap-0.5 overflow-y-auto rounded border p-1"
        data-testid="mod-position-results"
      >
        <li
          v-for="row in matches"
          :key="row.modId"
        >
          <button
            type="button"
            class="hover:bg-surface-2 focus-visible:bg-accent-soft flex w-full cursor-pointer items-center rounded px-2 py-1 text-left text-sm"
            :title="modLabel.titleFor(row.modId)"
            :data-testid="`mod-position-after-${row.modId}`"
            @click="choose({ kind: 'after', modId: row.modId })"
          >
            <span class="truncate">{{ modLabel.label(row.modId) }}</span>
          </button>
        </li>
        <li
          v-if="matches.length === 0"
          role="presentation"
          class="text-text-faint px-2 py-1 text-sm"
        >
          {{ t("mods.picker.noMatches") }}
        </li>
      </ul>
      <p
        v-if="isCapped"
        class="text-text-faint text-xs"
        data-testid="mod-position-capped"
      >
        {{ t("mods.move.pickCapped", { count: MAX_MATCHES }) }}
      </p>
    </div>
  </Dialog>
</template>
