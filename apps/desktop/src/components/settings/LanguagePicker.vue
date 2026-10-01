<script setup lang="ts">
import Select from "primevue/select";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { setAppLocale } from "@/i18n/i18n";
import { LOCALE_OPTIONS, type StoredLocale } from "@/i18n/locales";
import { useOpenAppLinkMutation } from "@/queries/links";
import { usePreferencesStore } from "@/stores/preferences";
import { detectSystemLocale } from "@/utils/detectSystemLocale";

/**
 * The language picker — shared by `SettingsPage.vue` and `SetupPage.vue`
 * (the same compact picker on both, since there is no shell and no
 * Settings route before a project loads). Applies **immediately** on
 * change, through {@link setAppLocale}: this is a per-machine viewing
 * preference (`stores/preferences.ts`), never part of a profile form's
 * own Save button.
 */

const { t } = useI18n();
const preferences = usePreferencesStore();
const { mutateAsync: openAppLink } = useOpenAppLinkMutation();

/**
 * `previewNote`'s own `{url}` placeholder is rendered as a real button
 * through `<i18n-t>`'s `#url` slot rather than interpolated as text —
 * this component never builds or sends a URL itself, only asks the
 * backend to open its one fixed issues-page link.
 */
function openIssuesLink(): void {
  void openAppLink("githubIssues");
}

type PickerOption = { value: StoredLocale; label: string; isPreview: boolean };

const options = computed<PickerOption[]>(() => [
  {
    value: "system",
    label: t("settings.language.system", { detected: detectSystemLocale().nativeName }),
    isPreview: false,
  },
  ...LOCALE_OPTIONS.map((option) => ({
    value: option.locale,
    label: option.isPreview
      ? t("settings.language.optionPreview", {
          name: option.nativeName,
          preview: t("settings.language.preview"),
        })
      : option.nativeName,
    isPreview: option.isPreview,
  })),
]);

/** Whether the *currently selected* option is a preview (unreviewed) locale — drives the note below the picker. */
const selectedIsPreview = computed(
  () => options.value.find((option) => option.value === preferences.locale)?.isPreview ?? false,
);

async function handleChange(value: StoredLocale): Promise<void> {
  await setAppLocale(value);
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-text font-medium">{{ t("settings.language.label") }}</span>
      <Select
        :model-value="preferences.locale"
        :options="options"
        option-label="label"
        option-value="value"
        data-testid="language-picker-select"
        @update:model-value="handleChange"
      />
    </label>
    <i18n-t
      v-if="selectedIsPreview"
      keypath="settings.language.previewNote"
      tag="p"
      class="text-text-muted text-xs"
      data-testid="language-picker-preview-note"
    >
      <template #url>
        <button
          type="button"
          class="text-accent cursor-pointer underline"
          data-testid="language-picker-issues-link"
          @click="openIssuesLink"
        >
          {{ t("settings.language.previewNoteLinkText") }}
        </button>
      </template>
    </i18n-t>
  </div>
</template>
