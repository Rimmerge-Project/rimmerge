<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { computed, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { CommandErrorDescriptor } from "@/utils/errors";
import { subtreeIds } from "@/utils/organization";

/**
 * Chooses where a group (with its subgroups) goes: the top level or inside another group. A
 * group cannot go inside itself or its own subgroups, and those targets are listed disabled
 * with the reason. The tree shown here only hints; the backend still refuses a cycle or a
 * level that is too deep, and the refusal shows in the dialog.
 */
const { group, nodes, error, isSubmitting } = defineProps<{
  /** The group being moved. */
  group: GroupNodeDto | null;
  /** Every group, depth first. */
  nodes: readonly GroupNodeDto[];
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{ move: [parent: number | null] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const legendId = useId();

/** The target the radios hold: a group id, or `top` for the top level. */
const picked = ref<number | "top" | null>(null);

watch(visible, (isOpen) => {
  if (isOpen) {
    picked.value = null;
  }
});

const blocked = computed(() => (group === null ? new Set<number>() : subtreeIds(nodes, group.id)));
const currentParent = computed(() => group?.parent ?? null);

function isTopLevelCurrent(): boolean {
  return currentParent.value === null;
}

function reasonFor(node: GroupNodeDto): string | null {
  if (blocked.value.has(node.id)) {
    return t("mods.groups.dialog.moveInvalid");
  }
  return node.id === currentParent.value ? t("mods.groups.dialog.moveCurrent") : null;
}

function submit(): void {
  if (picked.value !== null) {
    emit("move", picked.value === "top" ? null : picked.value);
  }
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('mods.groups.dialog.moveTitle')"
    :style="{ width: '28rem' }"
    data-testid="group-move-dialog"
  >
    <div
      v-if="group"
      class="flex flex-col gap-3 text-sm"
    >
      <p>
        <i18n-t
          keypath="mods.groups.dialog.moveQuestion"
          tag="span"
        >
          <template #name>
            <strong>{{ group.name }}</strong>
          </template>
        </i18n-t>
      </p>
      <fieldset
        role="radiogroup"
        :aria-labelledby="legendId"
        class="border-border-subtle bg-surface-1 flex max-h-72 flex-col gap-0.5 overflow-y-auto rounded border p-1"
      >
        <legend
          :id="legendId"
          class="sr-only"
        >
          {{ t("mods.groups.dialog.moveLegend") }}
        </legend>
        <label
          class="flex items-center gap-2 rounded px-2 py-1"
          :class="isTopLevelCurrent() ? 'text-text-faint' : 'hover:bg-surface-2 cursor-pointer'"
        >
          <input
            v-model="picked"
            type="radio"
            name="move-target"
            value="top"
            :disabled="isTopLevelCurrent()"
            data-testid="group-move-top"
          >
          <span>{{ t("mods.groups.dialog.topLevel") }}</span>
          <span
            v-if="isTopLevelCurrent()"
            class="text-xs"
          >{{ t("mods.groups.dialog.moveCurrent") }}</span>
        </label>
        <label
          v-for="node in nodes"
          :key="node.id"
          class="flex items-center gap-2 rounded px-2 py-1"
          :class="reasonFor(node) !== null ? 'text-text-faint' : 'hover:bg-surface-2 cursor-pointer'"
          :style="{ paddingInlineStart: `${(node.depth - 1) * 14 + 8}px` }"
        >
          <input
            v-model="picked"
            type="radio"
            name="move-target"
            :value="node.id"
            :disabled="reasonFor(node) !== null"
            :data-testid="`group-move-target-${node.id}`"
          >
          <span class="truncate">{{ node.name }}</span>
          <span
            v-if="reasonFor(node) !== null"
            class="text-xs"
            :data-testid="`group-move-reason-${node.id}`"
          >{{ reasonFor(node) }}</span>
        </label>
      </fieldset>
    </div>
    <OrganizationError
      v-if="error"
      class="mt-3"
      :error="error"
    />
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="group-move-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.groups.dialog.moveButton')"
        :loading="isSubmitting"
        :disabled="picked === null"
        data-testid="group-move-submit"
        @click="submit"
      />
    </template>
  </Dialog>
</template>
