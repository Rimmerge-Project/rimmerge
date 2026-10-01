<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import ModPicker from "@/components/mods/ModPicker.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { useUpdateAssignmentMutation } from "@/queries/assignments";
import { asRecord } from "@/types/dtoMaps";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentUpdateResultDto } from "@/types/generated/AssignmentUpdateResultDto";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { SchemaChangeDto } from "@/types/generated/SchemaChangeDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * Editing a project's R (reference mods) / T (target mods) after
 * creation: this component is the UI caller of
 * `useUpdateAssignmentMutation`. Changing either can silently drop stored
 * data — a
 * reclassified field's own stored value that no longer fits its new
 * role (`stranded_values`) and a row whose target left T
 * (`dropped_rows`) — both are disclosed here, plainly, right after the
 * update completes; this is a data-loss notice, not a nicety, the same
 * per-field disclosure convention `RowEditor.vue` already established
 * for `dropped`/`clampedFrom`.
 *
 * **Deliberately out of scope, disclosed rather than silently
 * half-built**: `excludedRefs` (the wizard's own "exclude a
 * closure-pulled-in dependency" toggle) is not editable here — doing
 * that honestly needs the wizard's own `effectiveRefs`/exclude-toggle UI
 * (`list_assignment_candidates`'s own closure computation), a genuinely
 * separate, larger feature than this dialog's own scope. A stored
 * `excludedRefs` value is preserved untouched by every save this dialog
 * makes (`excludedRefs: null` in the request means "leave unchanged",
 * never "clear it" — see `UpdateAssignmentRequestDto`'s own doc
 * comment). `name`/`author`/`description` are likewise not editable
 * here: this dialog's whole point is closing the R/T data-loss path, not
 * becoming a general project-settings editor.
 */
const { assignment, visible } = defineProps<{
  assignment: AssignmentDetailDto;
  visible: boolean;
}>();
const emit = defineEmits<{ "update:visible": [value: boolean] }>();
const { t, locale } = useI18n();
const tm = useTranslateMessage();

const refs = ref<ModRefDto[]>([]);
const targets = ref<ModRefDto[]>([]);
const saveError = ref<CommandErrorDescriptor | null>(null);
const lastOutcome = ref<AssignmentUpdateResultDto | null>(null);

// Re-seeds the draft from the project's own current R/T every time the
// dialog opens — never while it's already open (a background refetch
// mid-edit must not silently discard what the user is typing) — and
// clears any previous run's own outcome/error so a stale disclosure from
// last time never lingers into a fresh open.
watch(
  () => visible,
  (isVisible) => {
    if (!isVisible) {
      return;
    }
    refs.value = assignment.refs;
    targets.value = assignment.targets;
    saveError.value = null;
    lastOutcome.value = null;
  },
  { immediate: true },
);

const { mutateAsync: updateAssignment, isLoading: isSaving } = useUpdateAssignmentMutation();

async function save(): Promise<void> {
  saveError.value = null;
  try {
    lastOutcome.value = await updateAssignment({
      assignmentId: assignment.id,
      name: null,
      author: null,
      description: null,
      refs: refs.value.map((member) => member.modId),
      excludedRefs: null,
      targets: targets.value.map((member) => member.modId),
    });
  } catch (err) {
    saveError.value = describeCommandError(err);
  }
}

function close(): void {
  emit("update:visible", false);
}

/**
 * One `schemaChanges` row's own "N fields added, M removed, and K
 * reclassified" line — each count pluralizes on its own
 * (`schemaChangeCounts.{added,removed,reclassified}`), rather than one
 * message sharing a single elided noun across three numbers, which a
 * translation can't reorder or repluralize independently. The three
 * phrases are joined with `formatList` (`Intl.ListFormat`), never a
 * hard-coded `", "` — a translator can only ever place them in English
 * word order and with an English separator otherwise.
 */
function schemaChangeCountsText(change: SchemaChangeDto): string {
  const added = t(
    "assignments.editRefsTargets.schemaChangeCounts.added",
    { count: change.added.length },
    change.added.length,
  );
  const removed = t(
    "assignments.editRefsTargets.schemaChangeCounts.removed",
    { count: change.removed.length },
    change.removed.length,
  );
  const reclassified = t(
    "assignments.editRefsTargets.schemaChangeCounts.reclassified",
    { count: change.reclassified.length },
    change.reclassified.length,
  );
  return formatList(locale.value, [added, removed, reclassified]);
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :header="t('assignments.editRefsTargets.header')"
    class="w-[36rem]"
    data-testid="edit-refs-targets-dialog"
    @update:visible="(value: boolean) => emit('update:visible', value)"
  >
    <div class="flex flex-col gap-4">
      <section class="flex flex-col gap-1">
        <h3 class="text-text-muted text-xs font-semibold uppercase">
          {{ t("assignments.editRefsTargets.referencesHeading") }}
        </h3>
        <ModPicker
          :members="refs"
          :target-label="t('assignments.editRefsTargets.referencesHeading')"
          testid-prefix="edit-refs"
          data-testid="edit-refs-picker"
          @change="(members) => (refs = members)"
        />
      </section>

      <section class="flex flex-col gap-1">
        <h3 class="text-text-muted text-xs font-semibold uppercase">
          {{ t("assignments.editRefsTargets.targetsHeading") }}
        </h3>
        <ModPicker
          :members="targets"
          :target-label="t('assignments.editRefsTargets.targetsHeading')"
          testid-prefix="edit-targets"
          data-testid="edit-targets-picker"
          @change="(members) => (targets = members)"
        />
      </section>

      <Message
        v-if="saveError"
        severity="error"
        data-testid="edit-refs-targets-error"
      >
        <div class="font-medium">
          {{ tm(saveError.title) }}
        </div>
        <div>{{ tm(saveError.detail) }}</div>
        <div
          v-if="saveError.technicalDetail"
          class="text-xs opacity-75"
        >
          {{ saveError.technicalDetail }}
        </div>
      </Message>

      <section
        v-if="lastOutcome"
        class="flex flex-col gap-2"
        data-testid="edit-refs-targets-outcome"
      >
        <h3 class="text-text-muted text-xs font-semibold uppercase">
          {{ t("assignments.editRefsTargets.whatChangedHeading") }}
        </h3>
        <p
          v-for="[defType, change] in Object.entries(asRecord(lastOutcome.schemaChanges))"
          :key="defType"
          class="text-text text-xs"
          :data-testid="`schema-change-${defType}`"
        >
          <span class="font-mono">{{ defType }}</span>:
          {{ schemaChangeCountsText(change) }}
          <span v-if="change.reclassified.length > 0">
            {{
              t("assignments.editRefsTargets.reclassifiedNames", {
                names: formatList(locale, change.reclassified),
              })
            }}
          </span>
        </p>
        <div
          v-if="lastOutcome.strandedValues.length > 0"
          class="text-status-danger text-xs"
          data-testid="edit-refs-targets-stranded"
        >
          {{
            t(
              "assignments.editRefsTargets.strandedValuesLine",
              { count: lastOutcome.strandedValues.length },
              lastOutcome.strandedValues.length,
            )
          }}
          <ul class="list-inside list-disc">
            <li
              v-for="(value, index) in lastOutcome.strandedValues"
              :key="index"
            >
              <span class="font-mono">{{ value.defType }}</span> / {{ value.row }} / {{ value.path }}
            </li>
          </ul>
        </div>
        <div
          v-if="lastOutcome.droppedRows.length > 0"
          class="text-status-danger text-xs"
          data-testid="edit-refs-targets-dropped"
        >
          {{
            t(
              "assignments.editRefsTargets.droppedRowsLine",
              { count: lastOutcome.droppedRows.length },
              lastOutcome.droppedRows.length,
            )
          }}
          <ul class="list-inside list-disc">
            <li
              v-for="(row, index) in lastOutcome.droppedRows"
              :key="index"
            >
              <span class="font-mono">{{ row.defType }}</span>: {{ row.target.def.defType }}/{{ row.target.def.defName }}
            </li>
          </ul>
        </div>
        <p
          v-if="lastOutcome.strandedValues.length === 0 && lastOutcome.droppedRows.length === 0"
          class="text-status-auto text-xs"
          data-testid="edit-refs-targets-clean"
        >
          {{ t("assignments.editRefsTargets.clean") }}
        </p>
      </section>
    </div>

    <div class="mt-4 flex justify-end gap-2">
      <Button
        :label="t('assignments.editRefsTargets.closeButton')"
        size="small"
        severity="secondary"
        data-testid="edit-refs-targets-cancel"
        @click="close"
      />
      <Button
        :label="t('assignments.editRefsTargets.saveButton')"
        size="small"
        :loading="isSaving"
        data-testid="edit-refs-targets-save"
        @click="save"
      />
    </div>
  </Dialog>
</template>
