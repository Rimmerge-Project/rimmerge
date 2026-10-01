<script setup lang="ts">
import { refDebounced } from "@vueuse/core";
import InputText from "primevue/inputtext";
import { type ComponentPublicInstance, computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { useModsQuery } from "@/queries/mods";
import type { ModRefDto } from "@/types/generated/ModRefDto";

/**
 * A search-and-chips multi-select over the active mod list — the
 * assignment wizard's R/T pickers, following `components/patches/
 * ScopeEditor.vue`'s own pattern (search debounce, chip list, add/remove)
 * rather than reusing that component directly: a patch scope enforces a
 * minimum of two members and carries a patch-specific `scopeChange`
 * report, neither of which applies to a reference or target set (either
 * can be a single mod, with no analogous "shrink" summary).
 *
 * Lifted from `components/assignments/` to this already-shared `mods/`
 * directory: `components/rules/AddPlacementRuleDialog.vue`
 * reuses it too, for a single-mod pick — a mod's own bounded context has
 * nothing assignment-specific about it, so a second consumer outside
 * `assignments/` is a naming problem, not a design one. Reused
 * unmodified: a single pick is driven by always keeping `members` at
 * length 0 or 1 and replacing (never appending to) the previous
 * selection in the `change` handler — genuinely reusable behavior,
 * not `ScopeEditor.vue`'s own case of real per-caller differences that
 * argued against reuse in the first place (see above).
 */
const { members, testidPrefix, dataTestid, targetLabel } = defineProps<{
  /** The currently selected members. */
  members: ModRefDto[];
  /** The translated name of the field this picker fills, for the search box's accessible name. */
  targetLabel: string;
  /** Prefixes every per-row `data-testid` (`"refs"`/`"targets"`). */
  testidPrefix: string;
  /** The root element's own `data-testid`. */
  dataTestid: string;
}>();
const emit = defineEmits<{ change: [members: ModRefDto[]] }>();
const { t } = useI18n();

const search = ref("");
const debouncedSearch = refDebounced(search, 300);

const { data } = useModsQuery(() => ({
  search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
  tag: null,
  source: null,
  offset: 0,
  limit: 20,
}));

const memberIds = computed(() => new Set(members.map((member) => member.modId)));

const candidates = computed(() =>
  (data.value?.items ?? []).filter((mod) => !memberIds.value.has(mod.modId)),
);

function addMember(modId: string, name: string): void {
  emit("change", [...members, { modId, name }]);
}

function removeMember(modId: string): void {
  emit(
    "change",
    members.filter((member) => member.modId !== modId),
  );
}

const searchInputRef = useTemplateRef<ComponentPublicInstance>("searchInput");

/** Focuses the search box. */
function focusSearch(): void {
  const element = searchInputRef.value?.$el as HTMLElement | undefined;
  element?.focus();
}
defineExpose({ focusSearch });
</script>

<template>
  <div
    class="flex flex-col gap-2"
    :data-testid="dataTestid"
  >
    <div
      class="flex flex-wrap gap-1"
      role="list"
    >
      <span
        v-for="member in members"
        :key="member.modId"
        role="listitem"
        class="bg-surface-2 text-text flex items-center gap-1 rounded-full py-0.5 pr-1 pl-2 text-xs"
        :data-testid="`${testidPrefix}-member-${member.modId}`"
      >
        {{ member.name }}
        <button
          type="button"
          class="text-text-faint hover:text-status-danger cursor-pointer"
          :aria-label="t('mods.picker.removeMember', { name: member.name })"
          :data-testid="`${testidPrefix}-member-remove-${member.modId}`"
          @click="removeMember(member.modId)"
        >
          ×
        </button>
      </span>
      <span
        v-if="members.length === 0"
        class="text-text-faint text-xs"
      >{{ t("mods.picker.noneSelectedYet") }}</span>
    </div>

    <InputText
      ref="searchInput"
      v-model="search"
      :placeholder="t('mods.picker.searchPlaceholder')"
      :data-testid="`${testidPrefix}-search`"
      :aria-label="t('mods.picker.searchLabel', { target: targetLabel })"
    />
    <ul
      v-if="search.length > 0"
      class="border-border-subtle bg-surface-1 flex max-h-40 flex-col gap-0.5 overflow-y-auto rounded border p-1"
      :data-testid="`${testidPrefix}-search-results`"
    >
      <li
        v-for="mod in candidates"
        :key="mod.modId"
      >
        <button
          type="button"
          class="hover:bg-surface-2 flex w-full cursor-pointer items-center justify-between gap-2 rounded px-2 py-1 text-left text-xs"
          :data-testid="`${testidPrefix}-add-${mod.modId}`"
          @click="addMember(mod.modId, mod.name)"
        >
          <span>{{ mod.name }}</span>
          <span class="text-text-faint">{{ mod.modId }}</span>
        </button>
      </li>
      <li
        v-if="candidates.length === 0"
        role="presentation"
        class="text-text-faint px-2 py-1 text-xs"
      >
        {{ t("mods.picker.noMatches") }}
      </li>
    </ul>
  </div>
</template>
