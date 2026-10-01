<script setup lang="ts">
import { type MaybeRefOrGetter, toValue } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useModPreviewQuery } from "@/queries/mods";

const { modId } = defineProps<{ modId: MaybeRefOrGetter<string | null> }>();
const { t } = useI18n();
const modLabel = useModLabel();

const { data, isPending } = useModPreviewQuery(() => toValue(modId));
</script>

<template>
  <div
    class="bg-surface-2 aspect-video w-full shrink-0 overflow-hidden rounded"
    data-testid="mod-preview"
  >
    <div
      v-if="isPending"
      class="h-full w-full animate-pulse"
      data-testid="mod-preview-loading"
    />
    <img
      v-else-if="data?.kind === 'image'"
      :src="data.dataUrl"
      :alt="t('modInfo.panel.previewAlt', { mod: modLabel.label(toValue(modId) ?? '') })"
      class="h-full w-full object-contain"
      data-testid="mod-preview-image"
    >
    <p
      v-else-if="data?.kind === 'absent'"
      class="text-text-faint flex h-full items-center justify-center text-sm"
      data-testid="mod-preview-absent"
    >
      {{ t("modInfo.panel.noPreview") }}
    </p>
    <p
      v-else-if="data?.kind === 'unreadable'"
      class="text-text-faint flex h-full items-center justify-center px-2 text-center text-sm"
      data-testid="mod-preview-unreadable"
    >
      {{
        data.reason === "tooLarge"
          ? t("modInfo.panel.previewTooLarge")
          : data.reason === "unsupportedFormat"
            ? t("modInfo.panel.previewUnsupportedFormat")
            : t("modInfo.panel.previewUnreadable")
      }}
    </p>
  </div>
</template>
