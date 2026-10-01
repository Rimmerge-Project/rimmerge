<script setup lang="ts">
import { useI18n } from "vue-i18n";

import BaseKbd from "@/components/base/BaseKbd.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { AlternativeDto } from "@/types/generated/AlternativeDto";
import { edgeKindLabel } from "@/utils/edgeKind";
import { describeAction } from "@/utils/format";
import { rationaleText } from "@/utils/rationale";

const { alternatives } = defineProps<{ alternatives: AlternativeDto[] }>();
const { t, locale } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();
/** 1-based, matching the `1`..`9` keyboard shortcuts. */
const emit = defineEmits<{ pick: [oneBasedIndex: number] }>();
</script>

<template>
  <ul
    class="flex flex-col gap-1"
    data-testid="alternative-list"
    :aria-label="t('inbox.alternatives.ariaLabel')"
  >
    <li
      v-for="(alternative, index) in alternatives"
      :key="index"
      class="flex items-center gap-2"
    >
      <button
        type="button"
        class="border-border-subtle bg-surface-1 hover:bg-surface-2 flex flex-1 cursor-pointer items-center gap-2 rounded border px-2 py-1 text-left text-sm focus-visible:outline"
        :data-testid="`alternative-${index + 1}`"
        @click="emit('pick', index + 1)"
      >
        <BaseKbd
          v-if="index < 9"
          :label="String(index + 1)"
        />
        <span class="flex-1">{{
          tm(describeAction(alternative.action, modLabel.label, (kind) => tm(edgeKindLabel(kind))))
        }}</span>
      </button>
      <span class="text-text-muted text-xs">{{
        rationaleText(alternative.rationaleCode, t, modLabel.label, locale)
      }}</span>
    </li>
  </ul>
</template>
