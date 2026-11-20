<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { CommandErrorDescriptor } from "@/utils/errors";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS, COLOR_TOKENS } from "@/utils/organization";

/**
 * Picks one of the twelve palette colours, or none, for a group or a mod. Every colour is a
 * radio with its swatch and its name, so the name carries the choice and the swatch only
 * decorates it.
 */
const { subject, current, error, isSubmitting } = defineProps<{
  /** What is being coloured, for the title and help. */
  subject: "group" | "mod";
  /** The colour it has now, or `null` for none. */
  current: ColorTokenDto | null;
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{ choose: [color: ColorTokenDto | null] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const legendId = useId();
const picked = ref<ColorTokenDto | "none">(current ?? "none");

watch(visible, (isOpen) => {
  if (isOpen) {
    picked.value = current ?? "none";
  }
});

function apply(): void {
  emit("choose", picked.value === "none" ? null : picked.value);
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="subject === 'group' ? t('mods.colors.groupTitle') : t('mods.colors.modTitle')"
    :style="{ width: '26rem' }"
    data-testid="color-dialog"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p class="text-text-muted">
        {{ subject === "group" ? t("mods.colors.groupHelp") : t("mods.colors.modHelp") }}
      </p>
      <fieldset
        role="radiogroup"
        :aria-labelledby="legendId"
        class="grid grid-cols-2 gap-1"
      >
        <legend
          :id="legendId"
          class="sr-only"
        >
          {{ t("mods.colors.legend") }}
        </legend>
        <label
          v-for="token in COLOR_TOKENS"
          :key="token"
          class="hover:bg-surface-2 flex cursor-pointer items-center gap-2 rounded px-2 py-1"
        >
          <input
            v-model="picked"
            type="radio"
            name="mark-color"
            :value="token"
            :data-testid="`color-option-${token}`"
          >
          <span
            class="size-3 shrink-0 rounded-full"
            :class="COLOR_SWATCH_CLASS[token]"
            aria-hidden="true"
          />
          <span>{{ t(COLOR_NAME_KEY[token]) }}</span>
        </label>
        <label
          class="hover:bg-surface-2 col-span-2 flex cursor-pointer items-center gap-2 rounded px-2 py-1"
        >
          <input
            v-model="picked"
            type="radio"
            name="mark-color"
            value="none"
            data-testid="color-option-none"
          >
          <span>{{ t("mods.colors.none") }}</span>
        </label>
      </fieldset>
      <OrganizationError
        v-if="error"
        :error="error"
      />
    </div>
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="color-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.colors.apply')"
        :loading="isSubmitting"
        data-testid="color-apply"
        @click="apply"
      />
    </template>
  </Dialog>
</template>
