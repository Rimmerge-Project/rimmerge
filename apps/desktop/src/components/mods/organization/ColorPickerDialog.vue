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
const {
  subject,
  current,
  error,
  isSubmitting,
  scopeText = null,
} = defineProps<{
  /** What is being coloured, for the title and help. */
  subject: "group" | "mod" | "mods";
  /** The colour it has now, or `null` for none. */
  current: ColorTokenDto | null;
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
  /** For a bulk colour: "12 selected (3 not shown)", so the dialog says how many mods it acts on. */
  scopeText?: string | null;
}>();
const emit = defineEmits<{
  choose: [color: ColorTokenDto | null];
  /** The dialog has finished leaving the page. */
  afterHide: [];
}>();
const visible = defineModel<boolean>("visible", { required: true });

const TITLE_KEY = {
  group: "mods.colors.groupTitle",
  mod: "mods.colors.modTitle",
  mods: "mods.colors.modsTitle",
} as const satisfies Record<"group" | "mod" | "mods", string>;
const HELP_KEY = {
  group: "mods.colors.groupHelp",
  mod: "mods.colors.modHelp",
  mods: "mods.colors.modsHelp",
} as const satisfies Record<"group" | "mod" | "mods", string>;

const { t } = useI18n();
const legendId = useId();

/**
 * What the dialog starts on: a group or a single mod shows the colour it has now. A bulk colour
 * has no single current colour, so it starts with nothing picked and Set colour waits for a
 * choice: one click must never clear every selected mod's colour by accident.
 */
function initialChoice(): ColorTokenDto | "none" | null {
  return subject === "mods" ? null : (current ?? "none");
}

const picked = ref<ColorTokenDto | "none" | null>(initialChoice());

watch(visible, (isOpen) => {
  if (isOpen) {
    picked.value = initialChoice();
  }
});

function apply(): void {
  if (picked.value === null) {
    return;
  }
  emit("choose", picked.value === "none" ? null : picked.value);
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t(TITLE_KEY[subject])"
    :style="{ width: '26rem' }"
    data-testid="color-dialog"
    @after-hide="emit('afterHide')"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p
        v-if="scopeText !== null"
        data-testid="color-scope"
      >
        {{ scopeText }}
      </p>
      <p class="text-text-muted">
        {{ t(HELP_KEY[subject]) }}
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
        :disabled="picked === null"
        data-testid="color-apply"
        @click="apply"
      />
    </template>
  </Dialog>
</template>
