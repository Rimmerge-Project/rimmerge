<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { asRecord } from "@/types/dtoMaps";
import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import type { CommandErrorDescriptor } from "@/utils/errors";

const { plan, error, isSubmitting } = defineProps<{
  /** The plan to confirm; the body stays empty until the first plan arrives. */
  plan: ActivatePlanDto | null;
  /**
   * The last plan or commit failure. A failed plan opens the dialog with
   * `plan` null, so the error is the whole body and Confirm is disabled.
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
    :header="t('mods.page.activateDialogHeader')"
    data-testid="activate-confirm-dialog"
  >
    <div
      v-if="plan"
      class="flex w-96 flex-col gap-3 text-sm"
    >
      <div v-if="plan.toAdd.length > 0">
        <p class="text-text-muted">
          {{ t("mods.page.willActivateHeading") }}
        </p>
        <ul class="list-inside list-disc">
          <li
            v-for="id in plan.toAdd"
            :key="id"
            data-testid="activate-plan-to-add"
          >
            {{ modLabel.label(id) }}
          </li>
        </ul>
      </div>
      <div v-if="plan.alreadyActive.length > 0">
        <p class="text-text-muted">
          {{ t("mods.page.alreadyActiveHeading") }}
        </p>
        <ul class="list-inside list-disc">
          <li
            v-for="id in plan.alreadyActive"
            :key="id"
          >
            {{ modLabel.label(id) }}
          </li>
        </ul>
      </div>
      <Message
        v-for="(deps, id) in asRecord(plan.unresolvableDependencies)"
        :key="id"
        severity="warn"
        data-testid="activate-plan-unresolvable"
      >
        {{
          t("mods.page.unresolvableDependency", {
            mod: modLabel.label(id),
            deps: formatList(
              locale,
              deps.map((dep) => modLabel.label(dep)),
            ),
            count: deps.length,
          }, deps.length)
        }}
      </Message>
    </div>
    <Message
      v-if="error"
      class="w-96"
      severity="error"
      data-testid="activate-confirm-error"
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
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="activate-confirm-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.page.activateButton')"
        :loading="isSubmitting"
        :disabled="plan === null"
        data-testid="activate-confirm-submit"
        @click="emit('confirm')"
      />
    </template>
  </Dialog>
</template>
