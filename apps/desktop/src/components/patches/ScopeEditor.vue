<script setup lang="ts">
import { refDebounced } from "@vueuse/core";
import InputText from "primevue/inputtext";
import { type ComponentPublicInstance, computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { useModsQuery } from "@/queries/mods";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { ScopeChangeDto } from "@/types/generated/ScopeChangeDto";

const { members, scopeChange = null } = defineProps<{
  /** The patch's current scope members. */
  members: ModRefDto[];
  /**
   * The most recent `update_patch`'s own scope-change report, when the
   * last emitted `change` shrank the scope — `null` otherwise (including
   * before any change has been made). Rendered here, not just as a
   * header count, since it's specifically about *this* edit.
   */
  scopeChange?: ScopeChangeDto | null;
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

/**
 * Search results worth offering: never a mod already in scope, and never
 * a generated mod (Rimmerge's own merge/patch mods can't be a scope
 * member — `create_patch`/`update_patch` refuse them anyway; filtered
 * here too so the picker never even offers one). `== null`, not `===`,
 * catches a mock fixture that omits the field entirely as well as the
 * real backend's explicit `null`.
 */
const candidates = computed(() =>
  (data.value?.items ?? []).filter(
    (mod) => mod.generated == null && !memberIds.value.has(mod.modId),
  ),
);

/** `PatchScope::new`'s own floor — removing below it would leave `update_patch` rejecting the very edit this button just made. */
const MINIMUM_SIZE = 2;
const atMinimumSize = computed(() => members.length <= MINIMUM_SIZE);

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

/** Focuses the scope search box — the patch detail page's `s` shortcut. */
function focusSearch(): void {
  const element = searchInputRef.value?.$el as HTMLElement | undefined;
  element?.focus();
}
defineExpose({ focusSearch });
</script>

<template>
  <div
    class="flex flex-col gap-2"
    data-testid="scope-editor"
  >
    <div
      class="flex flex-wrap gap-1"
      role="list"
      :aria-label="t('patches.scopeEditor.membersListLabel')"
    >
      <span
        v-for="member in members"
        :key="member.modId"
        role="listitem"
        class="bg-surface-2 text-text flex items-center gap-1 rounded-full py-0.5 pr-1 pl-2 text-xs"
        :data-testid="`scope-member-${member.modId}`"
      >
        {{ member.name }}
        <button
          type="button"
          class="text-text-faint hover:text-status-danger cursor-pointer disabled:hover:text-text-faint disabled:cursor-not-allowed disabled:opacity-50"
          :aria-label="t('patches.scopeEditor.removeMember', { name: member.name })"
          :disabled="atMinimumSize"
          :title="atMinimumSize ? t('patches.scopeEditor.minimumSizeTitle') : undefined"
          :data-testid="`scope-member-remove-${member.modId}`"
          @click="removeMember(member.modId)"
        >
          ×
        </button>
      </span>
      <span
        v-if="members.length === 0"
        class="text-text-faint text-xs"
      >{{ t("patches.scopeEditor.noMembersYet") }}</span>
    </div>

    <InputText
      ref="searchInput"
      v-model="search"
      :placeholder="t('patches.scopeEditor.searchPlaceholder')"
      data-testid="scope-search"
      :aria-label="t('patches.scopeEditor.searchLabel')"
    />
    <ul
      v-if="search.length > 0"
      class="border-border-subtle bg-surface-1 flex max-h-40 flex-col gap-0.5 overflow-y-auto rounded border p-1"
      :aria-label="t('patches.scopeEditor.searchResultsLabel')"
      data-testid="scope-search-results"
    >
      <li
        v-for="mod in candidates"
        :key="mod.modId"
      >
        <button
          type="button"
          class="hover:bg-surface-2 flex w-full cursor-pointer items-center justify-between gap-2 rounded px-2 py-1 text-left text-xs"
          :data-testid="`scope-add-${mod.modId}`"
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
        {{ t("patches.scopeEditor.noMatches") }}
      </li>
    </ul>

    <div
      v-if="scopeChange && (scopeChange.nowOrphaned.length > 0 || scopeChange.choicesNamingRemoved.length > 0)"
      class="bg-status-input-soft text-status-input flex flex-col gap-1 rounded p-2 text-xs"
      data-testid="scope-change-summary"
    >
      <p class="font-medium">
        {{
          t(
            "patches.scopeEditor.scopeShrunk",
            { count: scopeChange.nowOrphaned.length },
            scopeChange.nowOrphaned.length,
          )
        }}
      </p>
      <ul class="list-inside list-disc">
        <li
          v-for="key in scopeChange.nowOrphaned"
          :key="key"
        >
          {{ key }}
        </li>
      </ul>
      <p v-if="scopeChange.choicesNamingRemoved.length > 0">
        {{
          t(
            "patches.scopeEditor.remainingChoicesNamingRemoved",
            { count: scopeChange.choicesNamingRemoved.length },
            scopeChange.choicesNamingRemoved.length,
          )
        }}
      </p>
    </div>
  </div>
</template>
