<script setup lang="ts">
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { SourceDto } from "@/types/generated/SourceDto";
import { sourceLabel } from "@/utils/source";

const { searchLabel } = defineProps<{
  /** The search box's accessible name, which says which list it filters (two lists sit side by side). */
  searchLabel: string;
}>();
const search = defineModel<string>("search", { required: true });
const tag = defineModel<string>("tag", { required: true });
const source = defineModel<SourceDto | null>("source", { required: true });

const { t } = useI18n();
const tm = useTranslateMessage();

const SOURCE_VALUES: readonly SourceDto[] = ["core", "dlc", "local", "workshop"];
/** `sourceLabel`'s own descriptors, resolved through `t()` reactively — plus the "Any" option, which isn't a real `SourceDto`. */
const sourceOptions = computed(() => [
  { label: t("mods.page.sourceAny"), value: null },
  ...SOURCE_VALUES.map((value) => ({ label: tm(sourceLabel(value)), value })),
]);
</script>

<template>
  <div class="flex shrink-0 flex-wrap items-center gap-2">
    <InputText
      v-model="search"
      class="min-w-0 flex-1 basis-full text-ellipsis @md:basis-auto"
      :placeholder="t('mods.page.searchPlaceholder')"
      :title="t('mods.page.searchPlaceholder')"
      :aria-label="searchLabel"
      data-testid="mods-search"
    />
    <InputText
      v-model="tag"
      class="min-w-0 flex-1 text-ellipsis"
      :placeholder="t('mods.page.tagFilterPlaceholder')"
      :title="t('mods.page.tagFilterPlaceholder')"
      :aria-label="t('mods.page.tagFilterLabel')"
      data-testid="mods-tag-filter"
    />
    <Select
      v-model="source"
      class="w-36 shrink-0"
      :options="sourceOptions"
      option-label="label"
      option-value="value"
      :placeholder="t('mods.page.sourcePlaceholder')"
      data-testid="mods-source-filter"
    />
  </div>
</template>
