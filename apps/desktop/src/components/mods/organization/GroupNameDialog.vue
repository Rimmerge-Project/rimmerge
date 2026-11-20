<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import { computed, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { CommandErrorDescriptor } from "@/utils/errors";

/**
 * The name field for a new group, a new subgroup or a rename. The backend checks the rules (length,
 * a taken sibling name), so a refusal arrives as `error`, worded by its code; this dialog only
 * keeps an empty name from being sent.
 */
const { mode, initialName, parentName, error, isSubmitting } = defineProps<{
  mode: "create" | "rename";
  /** The name to start from (the group own name for a rename, empty for a new group). */
  initialName: string;
  /** The parent the group goes inside, or `null` for the top level; only a create shows it. */
  parentName: string | null;
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{ submit: [name: string] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const inputId = useId();
const name = ref(initialName);

const title = computed(() => {
  if (mode === "rename") {
    return t("mods.groups.dialog.renameTitle");
  }
  return parentName === null
    ? t("mods.groups.dialog.createTitle")
    : t("mods.groups.dialog.subgroupTitle");
});

// A fresh field each time the dialog opens.
watch(visible, (isOpen) => {
  if (isOpen) {
    name.value = initialName;
  }
});

function submit(): void {
  const trimmed = name.value.trim();
  if (trimmed.length > 0) {
    emit("submit", trimmed);
  }
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="title"
    :style="{ width: '26rem' }"
    data-testid="group-name-dialog"
  >
    <form
      class="flex flex-col gap-3 text-sm"
      @submit.prevent="submit"
    >
      <p
        v-if="mode === 'create' && parentName !== null"
        class="text-text-muted"
      >
        {{ t("mods.groups.dialog.inside", { parent: parentName }) }}
      </p>
      <label
        :for="inputId"
        class="font-medium"
      >{{ t("mods.groups.dialog.nameLabel") }}</label>
      <InputText
        :id="inputId"
        v-model="name"
        autofocus
        maxlength="64"
        data-testid="group-name-input"
      />
      <p class="text-text-muted text-xs">
        {{ t("mods.groups.dialog.nameHelp") }}
      </p>
      <OrganizationError
        v-if="error"
        :error="error"
      />
    </form>
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="group-name-cancel"
        @click="visible = false"
      />
      <Button
        :label="
          mode === 'rename' ? t('mods.groups.dialog.renameButton') : t('mods.groups.dialog.createButton')
        "
        :loading="isSubmitting"
        :disabled="name.trim().length === 0"
        data-testid="group-name-submit"
        @click="submit"
      />
    </template>
  </Dialog>
</template>
