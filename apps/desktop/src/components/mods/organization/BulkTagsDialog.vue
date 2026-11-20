<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import { useToast } from "primevue/usetoast";
import { computed, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import { useSetManualTagsMutation } from "@/queries/tags";
import type { TagModeDto } from "@/types/generated/TagModeDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { isTagSlug } from "@/utils/tags";

/**
 * Adds or removes one manual tag on the selected mods of a list with one backend call (one
 * save). Removing takes the tag off every selected mod that shows it, added by hand or inferred;
 * mods that do not show it are left alone (the backend counts them as unchanged). It never
 * changes the selection, the working set or the load order: tags are not a sorter input. The
 * tag must already be a valid slug; the dialog does not rewrite what was typed.
 */
const { modIds, selectionText } = defineProps<{
  /** The selected mods of one list, hidden-by-filter ones included. */
  modIds: readonly string[];
  /** "12 selected (3 not shown)". */
  selectionText: string;
}>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const toast = useToast();
const inputId = useId();
const helpId = useId();
const { mutateAsync, isLoading } = useSetManualTagsMutation();

const tag = ref("");
const error = ref<CommandErrorDescriptor | null>(null);

watch(visible, (isOpen) => {
  if (isOpen) {
    tag.value = "";
    error.value = null;
  }
});

const isValid = computed(() => isTagSlug(tag.value));

async function submit(mode: TagModeDto): Promise<void> {
  if (!isValid.value || isLoading.value) {
    return;
  }
  error.value = null;
  const name = tag.value;
  try {
    const outcome = await mutateAsync({ modIds: [...modIds], tag: name, mode });
    const summary =
      mode === "add"
        ? t("mods.bulk.toast.tagAdded", { tag: name, count: outcome.changed }, outcome.changed)
        : t("mods.bulk.toast.tagRemoved", { tag: name, count: outcome.changed }, outcome.changed);
    const detail = [
      outcome.keptByDecision > 0
        ? t(
            "mods.bulk.toast.keptByDecision",
            { count: outcome.keptByDecision },
            outcome.keptByDecision,
          )
        : "",
      outcome.unchanged > 0
        ? t("mods.bulk.toast.unchanged", { count: outcome.unchanged }, outcome.unchanged)
        : "",
    ]
      .filter((line) => line !== "")
      .join("\n");
    toast.add({
      severity: "success",
      summary,
      ...(detail === "" ? {} : { detail }),
      life: 4000,
    });
    visible.value = false;
  } catch (caught: unknown) {
    error.value = describeCommandError(caught);
  }
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('mods.bulk.tags.title')"
    :style="{ width: '28rem' }"
    data-testid="bulk-tags-dialog"
  >
    <form
      class="flex flex-col gap-3 text-sm"
      @submit.prevent="submit('add')"
    >
      <p data-testid="bulk-tags-selection">
        {{ selectionText }}
      </p>
      <p class="text-text-muted">
        {{ t("mods.bulk.tags.help") }}
      </p>
      <label
        :for="inputId"
        class="font-medium"
      >{{ t("mods.bulk.tags.label") }}</label>
      <InputText
        :id="inputId"
        v-model="tag"
        autofocus
        :aria-describedby="helpId"
        :invalid="tag !== '' && !isValid"
        data-testid="bulk-tags-input"
      />
      <p
        :id="helpId"
        class="text-xs"
        :class="tag !== '' && !isValid ? 'text-status-danger' : 'text-text-muted'"
        data-testid="bulk-tags-format"
      >
        {{ t("mods.bulk.tags.format") }}
      </p>
      <OrganizationError
        v-if="error"
        :error="error"
      />
    </form>
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="bulk-tags-cancel"
        @click="visible = false"
      />
      <Button
        :label="t('mods.bulk.tags.removeButton')"
        severity="secondary"
        outlined
        :loading="isLoading"
        :disabled="!isValid"
        data-testid="bulk-tags-remove"
        @click="submit('remove')"
      />
      <Button
        :label="t('mods.bulk.tags.addButton')"
        :loading="isLoading"
        :disabled="!isValid"
        data-testid="bulk-tags-add"
        @click="submit('add')"
      />
    </template>
  </Dialog>
</template>
