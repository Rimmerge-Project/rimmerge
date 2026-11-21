<script setup lang="ts">
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import { computed, ref, useId } from "vue";
import { useI18n } from "vue-i18n";

import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { newGroupNameFor } from "@/utils/organization";

/**
 * The search field of an "Add to group" dialog. Typing filters the group list (the dialog does the
 * filtering with the same `query`); a name no group has offers "Create group", with an optional
 * parent (top level by default). The create button only says what to create: the dialog runs the
 * one backend call that creates the group and adds the mods together.
 */
const { nodes, isBusy, target } = defineProps<{
  /** Every group, depth first. */
  nodes: readonly GroupNodeDto[];
  isBusy: boolean;
  /** Whose mods the new group takes in: the bulk selection or the one mod of the dialog. */
  target: "selection" | "single";
}>();
const emit = defineEmits<{ create: [name: string, parent: number | null] }>();
const query = defineModel<string>("query", { required: true });

const { t } = useI18n();
const searchId = useId();
const parentId = useId();
const parent = ref<number | null>(null);

const newName = computed(() => newGroupNameFor(nodes, query.value));

/** A parent that no longer exists (the list changed) falls back to the top level. */
const chosenParent = computed(() =>
  parent.value !== null && nodes.some((node) => node.id === parent.value) ? parent.value : null,
);

function onCreate(): void {
  if (newName.value !== null && !isBusy) {
    emit("create", newName.value, chosenParent.value);
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <label
      :for="searchId"
      class="sr-only"
    >{{ t("mods.groups.search.label") }}</label>
    <InputText
      :id="searchId"
      v-model="query"
      :placeholder="t('mods.groups.search.placeholder')"
      autocomplete="off"
      maxlength="64"
      data-testid="group-search-input"
    />
    <div
      v-if="newName !== null"
      class="border-border-subtle bg-surface-1 flex flex-col gap-2 rounded border p-2"
      data-testid="group-create-option"
    >
      <Button
        :label="t('mods.groups.search.createOption', { name: newName })"
        icon="pi pi-plus"
        severity="secondary"
        size="small"
        class="self-start"
        :aria-disabled="isBusy"
        data-testid="group-create-button"
        @click="onCreate"
      />
      <p class="text-text-muted text-xs">
        {{ target === "single" ? t("mods.groups.search.createHintSingle") : t("mods.groups.search.createHintBulk") }}
      </p>
      <div class="flex items-center gap-2">
        <label
          :for="parentId"
          class="text-xs"
        >{{ t("mods.groups.search.parentLabel") }}</label>
        <select
          :id="parentId"
          v-model="parent"
          class="border-border-subtle bg-surface-2 min-w-0 flex-1 rounded border px-2 py-1 text-xs"
          data-testid="group-create-parent"
        >
          <option :value="null">
            {{ t("mods.groups.dialog.topLevel") }}
          </option>
          <option
            v-for="node in nodes"
            :key="node.id"
            :value="node.id"
          >
            {{ `${"\u00A0\u00A0".repeat(node.depth - 1)}${node.name}` }}
          </option>
        </select>
      </div>
    </div>
  </div>
</template>
