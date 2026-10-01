<script setup lang="ts">
import Button from "primevue/button";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { OrphanedDecisionDto } from "@/types/generated/OrphanedDecisionDto";
import { edgeKindLabel } from "@/utils/edgeKind";
import { describeAction } from "@/utils/format";

const { decisions } = defineProps<{ decisions: OrphanedDecisionDto[] }>();
const { t } = useI18n();
const modLabel = useModLabel();
const tm = useTranslateMessage();
const emit = defineEmits<{ prune: [key: string] }>();
</script>

<template>
  <div data-testid="orphaned-decisions">
    <p
      v-if="decisions.length === 0"
      class="text-text-muted text-sm"
      data-testid="orphaned-decisions-empty"
    >
      {{ t("rules.orphaned.empty") }}
    </p>
    <ul
      v-else
      class="flex flex-col gap-1"
    >
      <li
        v-for="decision in decisions"
        :key="decision.key"
        class="border-border-subtle bg-surface-1 flex items-center justify-between gap-2 rounded border px-2 py-1 text-sm"
        :data-testid="`orphaned-decision-${decision.key}`"
      >
        <div class="min-w-0">
          <div
            class="text-text-faint truncate font-mono text-[11px]"
            :title="decision.key"
          >
            {{ decision.key }}
          </div>
          <div class="text-text">
            {{ tm(describeAction(decision.action, modLabel.label, (kind) => tm(edgeKindLabel(kind)))) }}
          </div>
          <div
            v-if="decision.note"
            class="text-text-muted text-xs"
          >
            {{ decision.note }}
          </div>
        </div>
        <Button
          :label="t('rules.orphaned.pruneButton')"
          size="small"
          severity="danger"
          text
          :data-testid="`prune-${decision.key}`"
          @click="emit('prune', decision.key)"
        />
      </li>
    </ul>
  </div>
</template>
