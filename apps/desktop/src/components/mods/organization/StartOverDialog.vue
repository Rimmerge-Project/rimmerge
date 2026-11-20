<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { CommandErrorDescriptor } from "@/utils/errors";

/**
 * The confirmation for "Start over" after groups and colours could not be read: a copy of the
 * unreadable file is kept next to it, then Rimmerge starts with no groups and no colours.
 */
const { error, isSubmitting } = defineProps<{
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{ confirm: [] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('mods.groups.startOver.title')"
    :style="{ width: '28rem' }"
    data-testid="start-over-dialog"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p>{{ t("mods.groups.startOver.body") }}</p>
      <OrganizationError
        v-if="error"
        :error="error"
      />
    </div>
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="start-over-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.groups.startOver.confirm')"
        severity="danger"
        :loading="isSubmitting"
        data-testid="start-over-confirm"
        @click="emit('confirm')"
      />
    </template>
  </Dialog>
</template>
