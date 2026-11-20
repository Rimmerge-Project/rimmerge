<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { DeleteModeDto } from "@/types/generated/DeleteModeDto";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { CommandErrorDescriptor } from "@/utils/errors";

/**
 * The confirmation before a group is deleted. It names what happens to the subgroups (keep them
 * one level up, the default, or delete them too) and says that no mod is deleted or uninstalled.
 */
const { group, error, isSubmitting } = defineProps<{
  /** The group to delete; the body stays empty until there is one. */
  group: GroupNodeDto | null;
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{
  confirm: [mode: DeleteModeDto];
  /** The dialog has finished leaving the page, whichever way it closed. */
  afterHide: [];
}>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const legendId = useId();
const mode = ref<DeleteModeDto>("liftChildren");

watch(visible, (isOpen) => {
  if (isOpen) {
    mode.value = "liftChildren";
  }
});
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('mods.groups.dialog.deleteTitle')"
    :style="{ width: '28rem' }"
    data-testid="group-delete-dialog"
    @after-hide="emit('afterHide')"
  >
    <div
      v-if="group"
      class="flex flex-col gap-3 text-sm"
    >
      <p>
        <i18n-t
          keypath="mods.groups.dialog.deleteQuestion"
          tag="span"
        >
          <template #name>
            <strong>{{ group.name }}</strong>
          </template>
        </i18n-t>
      </p>
      <p class="text-text-muted">
        {{
          group.scopeCount === 0
            ? t("mods.groups.dialog.deleteNoMods")
            : t("mods.groups.dialog.deleteMods", { count: group.scopeCount }, group.scopeCount)
        }}
      </p>
      <fieldset
        v-if="group.childCount > 0"
        role="radiogroup"
        :aria-labelledby="legendId"
        class="flex flex-col gap-2"
      >
        <legend
          :id="legendId"
          class="font-medium"
        >
          {{ t("mods.groups.dialog.modeLegend", { count: group.childCount }, group.childCount) }}
        </legend>
        <label class="flex items-start gap-2">
          <input
            v-model="mode"
            type="radio"
            name="delete-mode"
            value="liftChildren"
            class="mt-0.5"
            data-testid="group-delete-lift"
          >
          <span>{{ t("mods.groups.dialog.keepSubgroups") }}</span>
        </label>
        <label class="flex items-start gap-2">
          <input
            v-model="mode"
            type="radio"
            name="delete-mode"
            value="withSubgroups"
            class="mt-0.5"
            data-testid="group-delete-with-subgroups"
          >
          <span>{{ t("mods.groups.dialog.deleteSubgroups") }}</span>
        </label>
      </fieldset>
      <p
        class="text-text-muted"
        data-testid="group-delete-safety"
      >
        {{ t("mods.groups.dialog.deleteSafety") }}
      </p>
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
        data-testid="group-delete-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.groups.dialog.deleteButton')"
        severity="danger"
        :loading="isSubmitting"
        :disabled="group === null"
        data-testid="group-delete-submit"
        @click="emit('confirm', mode)"
      />
    </template>
  </Dialog>
</template>
