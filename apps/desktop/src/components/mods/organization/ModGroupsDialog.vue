<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { CommandErrorDescriptor } from "@/utils/errors";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS } from "@/utils/organization";

/**
 * One mod groups: every group as a checkbox, checked while the mod is a direct member. A click
 * saves at once (one backend call, one save), and the checkbox follows what was saved, so a
 * refusal leaves it as it was and shows the error. Adding never removes the mod from another
 * group. While a save runs the boxes are `aria-disabled` rather than `disabled`: a disabled
 * element drops keyboard focus, and the user would lose their place in the list.
 */
const { modName, nodes, memberOf, error, isBusy } = defineProps<{
  /** The mod as the lists label it. */
  modName: string;
  /** Every group, depth first. */
  nodes: readonly GroupNodeDto[];
  /** The groups the mod is a direct member of, as saved. */
  memberOf: readonly number[];
  error: CommandErrorDescriptor | null;
  isBusy: boolean;
}>();
const emit = defineEmits<{ toggle: [groupId: number, isMember: boolean] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const members = computed(() => new Set(memberOf));

/** The click is the request; the box only changes once the new membership is saved. */
function onClick(event: MouseEvent, node: GroupNodeDto): void {
  event.preventDefault();
  if (isBusy) {
    return;
  }
  emit("toggle", node.id, !members.value.has(node.id));
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('mods.groups.dialog.modGroupsTitle', { name: modName })"
    :style="{ width: '28rem' }"
    data-testid="mod-groups-dialog"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p class="text-text-muted">
        {{ t("mods.groups.dialog.modGroupsHelp") }}
      </p>
      <p
        v-if="nodes.length === 0"
        data-testid="mod-groups-empty"
      >
        {{ t("mods.groups.dialog.modGroupsEmpty") }}
      </p>
      <ul
        v-else
        class="border-border-subtle bg-surface-1 flex max-h-72 flex-col gap-0.5 overflow-y-auto rounded border p-1"
        :aria-label="t('mods.groups.dialog.modGroupsList')"
      >
        <li
          v-for="node in nodes"
          :key="node.id"
        >
          <label
            class="hover:bg-surface-2 flex cursor-pointer items-center gap-2 rounded px-2 py-1"
            :style="{ paddingInlineStart: `${(node.depth - 1) * 14 + 8}px` }"
          >
            <input
              type="checkbox"
              :checked="members.has(node.id)"
              :aria-disabled="isBusy"
              :data-testid="`mod-groups-toggle-${node.id}`"
              @click="onClick($event, node)"
            >
            <span
              v-if="node.color !== null"
              class="size-3 shrink-0 rounded-full"
              :class="COLOR_SWATCH_CLASS[node.color]"
              :title="t(COLOR_NAME_KEY[node.color])"
              aria-hidden="true"
            />
            <span class="truncate">{{ node.name }}</span>
          </label>
        </li>
      </ul>
      <OrganizationError
        v-if="error"
        :error="error"
      />
    </div>
    <template #footer>
      <Button
        :label="t('mods.groups.dialog.done')"
        data-testid="mod-groups-done"
        @click="visible = false"
      />
    </template>
  </Dialog>
</template>
