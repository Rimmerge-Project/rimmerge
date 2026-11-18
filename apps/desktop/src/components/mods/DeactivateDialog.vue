<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { asRecord } from "@/types/dtoMaps";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";
import type { CommandErrorDescriptor } from "@/utils/errors";

const { plan, error, isSubmitting } = defineProps<{
  /** The plan to confirm; the body stays empty until the first plan arrives. */
  plan: DeactivatePlanDto | null;
  /**
   * The last commit failure. A plan failure is currently not shown: the
   * dialog never opens on one (tracked in the milestone plan, fixed in
   * batch B).
   */
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{ confirm: [] }>();
const visible = defineModel<boolean>("visible", { required: true });

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('mods.page.deactivateDialogHeader')"
    data-testid="deactivate-confirm-dialog"
  >
    <div
      v-if="plan"
      class="flex w-96 flex-col gap-3 text-sm"
    >
      <div v-if="plan.toRemove.length > 0">
        <p class="text-text-muted">
          {{ t("mods.page.willDeactivateHeading") }}
        </p>
        <ul class="list-inside list-disc">
          <li
            v-for="id in plan.toRemove"
            :key="id"
            data-testid="deactivate-plan-to-remove"
          >
            {{ modLabel.label(id) }}
          </li>
        </ul>
      </div>
      <div v-if="plan.refused.length > 0">
        <p class="text-text-muted">
          {{ t("mods.page.refusedHeading") }}
        </p>
        <ul class="list-inside list-disc">
          <li
            v-for="id in plan.refused"
            :key="id"
          >
            {{ modLabel.label(id) }}
          </li>
        </ul>
      </div>
      <Message
        v-for="(dependents, id) in asRecord(plan.dependentsStillActive)"
        :key="id"
        severity="warn"
        data-testid="deactivate-plan-dependents"
      >
        {{
          t(
            "mods.page.dependentsStillActive",
            {
              count: dependents.length,
              deps: formatList(
                locale,
                dependents.map((dep) => modLabel.label(dep)),
              ),
              mod: modLabel.label(id),
            },
            dependents.length,
          )
        }}
      </Message>
      <Message
        v-if="error"
        severity="error"
        data-testid="deactivate-confirm-error"
      >
        <div class="font-medium">
          {{ tm(error.title) }}
        </div>
        <div>{{ tm(error.detail) }}</div>
        <div
          v-if="error.technicalDetail"
          class="text-xs opacity-75"
        >
          {{ error.technicalDetail }}
        </div>
      </Message>
    </div>
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="deactivate-confirm-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.page.deactivateButton')"
        severity="danger"
        :loading="isSubmitting"
        data-testid="deactivate-confirm-submit"
        @click="emit('confirm')"
      />
    </template>
  </Dialog>
</template>
