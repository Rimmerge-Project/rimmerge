<script setup lang="ts">
// The "Apply first?" body: why ModsConfig.xml is behind, and the three ways out. Cancel is the
// focused default (the same convention as `ApplyConfirm.vue`'s "Go back"), so a stray Enter
// backs out instead of applying or launching.
import Button from "primevue/button";
import { nextTick, onMounted, useId, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

defineProps<{ sentence: string }>();
const emit = defineEmits<{ applyFirst: []; launchAnyway: []; cancel: [] }>();

const { t } = useI18n();
const sentenceId = useId();
const cancelButton = useTemplateRef<{ $el: HTMLElement }>("cancelButton");

onMounted(async () => {
  await nextTick();
  cancelButton.value?.$el.focus();
});
</script>

<template>
  <div
    class="flex w-96 max-w-full flex-col gap-4"
    role="group"
    :aria-describedby="sentenceId"
    data-testid="launch-apply-first"
  >
    <p
      :id="sentenceId"
      class="text-text text-sm"
      data-testid="launch-apply-first-sentence"
    >
      {{ sentence }}
    </p>
    <div class="flex flex-wrap justify-end gap-2">
      <Button
        ref="cancelButton"
        :label="t('common.cancel')"
        severity="secondary"
        class="whitespace-nowrap"
        autofocus
        data-testid="launch-apply-first-cancel"
        @click="emit('cancel')"
      />
      <Button
        :label="t('gameLaunch.applyFirst.launchAnywayButton')"
        severity="secondary"
        class="whitespace-nowrap"
        data-testid="launch-apply-first-anyway"
        @click="emit('launchAnyway')"
      />
      <Button
        :label="t('gameLaunch.applyFirst.applyButton')"
        class="whitespace-nowrap"
        data-testid="launch-apply-first-apply"
        @click="emit('applyFirst')"
      />
    </div>
  </div>
</template>
