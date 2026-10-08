<script setup lang="ts">
// The "Paste a list…" dialog: a textarea for the "Copy as text" format or one package id per
// line. The text is previewed by the backend, never interpreted here.
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Textarea from "primevue/textarea";
import { computed, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

const { isBusy } = defineProps<{ isBusy: boolean }>();
const visible = defineModel<boolean>("visible", { required: true });
const emit = defineEmits<{ preview: [text: string] }>();

const { t } = useI18n();
const labelId = useId();
const hintId = useId();
const text = ref("");
const isEmpty = computed(() => text.value.trim() === "");

// A dialog reopened later starts empty; a list that was previewed and cancelled is gone.
watch(visible, (isVisible) => {
  if (!isVisible) {
    text.value = "";
  }
});

function submit(): void {
  if (isEmpty.value || isBusy) {
    return;
  }
  emit("preview", text.value);
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('orderShare.import.pasteTitle')"
    :closable="!isBusy"
    :close-on-escape="!isBusy"
    class="w-[36rem] max-w-[95vw]"
    data-testid="paste-order-dialog"
  >
    <form
      class="flex flex-col gap-3"
      @submit.prevent="submit"
    >
      <label
        :id="labelId"
        class="text-text text-sm font-medium"
        :for="`${labelId}-input`"
      >
        {{ t("orderShare.import.pasteLabel") }}
      </label>
      <Textarea
        :id="`${labelId}-input`"
        v-model="text"
        rows="12"
        :aria-describedby="hintId"
        class="font-mono text-sm"
        spellcheck="false"
        autofocus
        data-testid="paste-order-text"
      />
      <p
        :id="hintId"
        class="text-text-muted text-sm"
      >
        {{ t("orderShare.import.pasteHint") }}
      </p>
      <div class="flex justify-end gap-2">
        <Button
          :label="t('common.cancel')"
          severity="secondary"
          :disabled="isBusy"
          data-testid="paste-order-cancel"
          @click="visible = false"
        />
        <Button
          type="submit"
          :label="t('orderShare.import.preview')"
          :disabled="isEmpty"
          :loading="isBusy"
          data-testid="paste-order-preview"
        />
      </div>
    </form>
  </Dialog>
</template>
