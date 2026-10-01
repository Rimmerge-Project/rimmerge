<script setup lang="ts">
import Button from "primevue/button";
import Textarea from "primevue/textarea";
import { ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

/** The persisted note, if any — reset into the draft whenever it changes underneath. */
const { note } = defineProps<{ note: string | null }>();
const { t } = useI18n();
const emit = defineEmits<{ save: [note: string]; cancel: [] }>();

const draft = ref(note ?? "");
watch(
  () => note,
  (value) => {
    draft.value = value ?? "";
  },
);

// A stable, unique id per instance — a hardcoded id would collide if
// this component ever renders more than once at a time (e.g. two open
// notes, or a future list view).
const noteInputId = useId();
</script>

<template>
  <div
    class="flex flex-col gap-2"
    data-testid="decision-note"
  >
    <label
      class="text-text-muted text-xs font-medium"
      :for="noteInputId"
    >{{ t("inbox.decisionNote.label") }}</label>
    <Textarea
      :id="noteInputId"
      v-model="draft"
      rows="3"
      auto-resize
      data-testid="decision-note-input"
    />
    <div class="flex gap-2">
      <Button
        :label="t('inbox.decisionNote.save')"
        size="small"
        data-testid="decision-note-save"
        @click="emit('save', draft)"
      />
      <Button
        :label="t('common.cancel')"
        size="small"
        severity="secondary"
        data-testid="decision-note-cancel"
        @click="emit('cancel')"
      />
    </div>
  </div>
</template>
