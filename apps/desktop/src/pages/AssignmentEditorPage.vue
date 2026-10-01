<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import RadioButton from "primevue/radiobutton";
import Tab from "primevue/tab";
import TabList from "primevue/tablist";
import Tabs from "primevue/tabs";
import { computed, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import AssignmentExportPanel from "@/components/assignments/AssignmentExportPanel.vue";
import CoverageList from "@/components/assignments/CoverageList.vue";
import EditReferencesTargetsDialog from "@/components/assignments/EditReferencesTargetsDialog.vue";
import RowEditor from "@/components/assignments/RowEditor.vue";
import { useAssignmentEditorKeys } from "@/composables/useAssignmentEditorKeys";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import {
  useAddAssignmentSectionMutation,
  useAssignmentCoverageQuery,
  useAssignmentQuery,
  useDeleteAssignmentMutation,
  useListAssignmentCandidatesMutation,
  useRemoveAssignmentSectionMutation,
} from "@/queries/assignments";
import { RimmergeError } from "@/services/ipc";
import type { CandidateSummaryDto } from "@/types/generated/CandidateSummaryDto";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";
import type { SectionReferenceDto } from "@/types/generated/SectionReferenceDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The patch maker's editor: one tab per section, in
 * `sections` order (the backend's own `BTreeMap` order). A target-keyed
 * section keeps the original three-pane layout (coverage queue, row
 * editor, existing matches); a free-standing ("new def") section has no
 * coverage queue at all — just its own rows, listed by `defName` (no
 * creation/edit UI yet post-creation, a disclosed, carried gap).
 */
const { t } = useI18n();
const tm = useTranslateMessage();
const route = useRoute();
const router = useRouter();

const assignmentId = computed(() => String(route.params["assignmentId"] ?? ""));
const { data: assignment, isPending, error } = useAssignmentQuery(assignmentId);

const sections = computed(() => assignment.value?.sections ?? []);
const selectedSectionType = ref<string | null>(null);

// Defaults to the first section once the project loads, and snaps back
// to it whenever the currently selected section stops existing (removed,
// or the project just loaded/reloaded) — never left pointing at a tab
// that's no longer there.
watch(
  sections,
  (list) => {
    if (list.length === 0) {
      selectedSectionType.value = null;
      return;
    }
    if (!list.some((section) => section.defType === selectedSectionType.value)) {
      selectedSectionType.value = list[0]?.defType ?? null;
    }
  },
  { immediate: true },
);

const selectedSection = computed(() =>
  sections.value.find((section) => section.defType === selectedSectionType.value),
);

// Coverage has no meaning for a free-standing section (`CoverageDto.applicable`
// would just come back `false`) — skip fetching it at all rather than
// firing a request the pane never renders.
const coverageSectionType = computed(() =>
  selectedSection.value && !selectedSection.value.isStandalone
    ? selectedSection.value.defType
    : null,
);
const { data: coverage } = useAssignmentCoverageQuery(assignmentId, coverageSectionType);

const selectedTarget = ref<CoverageRowDto["target"] | null>(null);
const selectedRow = computed(() =>
  (coverage.value?.rows ?? []).find(
    (row) =>
      row.target.def.defType === selectedTarget.value?.def.defType &&
      row.target.def.defName === selectedTarget.value?.def.defName,
  ),
);

// A tab switch starts the selection over — the previous section's
// selected target means nothing in a different section's own queue.
watch(coverageSectionType, () => {
  selectedTarget.value = null;
});

watch(
  () => coverage.value?.rows,
  (rows) => {
    if (selectedTarget.value || !rows || rows.length === 0) {
      return;
    }
    selectedTarget.value = rows[0]?.target ?? null;
  },
  { immediate: true },
);

function selectRow(row: CoverageRowDto): void {
  selectedTarget.value = row.target;
}

// --- Free-standing ("own") rows -----------------------------------------

/** A free-standing section's own row currently open for editing, by its own `defName` — `null` while nothing is selected and no new row is being created. */
const selectedOwnDefName = ref<string | null>(null);
/** Whether "New row" was clicked — a brand-new, unsaved row (`RowEditor`'s own `own-def-name="null"` mode), distinct from `selectedOwnDefName === null` meaning "nothing chosen at all" (no editor shown yet). */
const isCreatingOwnRow = ref(false);

// A tab switch (or the section list changing shape) starts the
// free-standing selection over too — the previous section's selected row
// means nothing in a different section.
watch(selectedSectionType, () => {
  selectedOwnDefName.value = null;
  isCreatingOwnRow.value = false;
});

function selectOwnRow(defName: string): void {
  isCreatingOwnRow.value = false;
  selectedOwnDefName.value = defName;
}

function startNewOwnRow(): void {
  selectedOwnDefName.value = null;
  isCreatingOwnRow.value = true;
}

/** `RowEditor`'s own `ownRowSaved` — a brand-new row becomes the selected, now-persisted one; an edit to an already-selected row just confirms the same selection (a no-op reassignment when the defName didn't change). */
function onOwnRowSaved(defName: string): void {
  isCreatingOwnRow.value = false;
  selectedOwnDefName.value = defName;
}

/** `RowEditor`'s own `ownRowCleared` — nothing left to edit under the old defName, so fall back to the empty "nothing selected" state rather than pointing at a row that no longer exists. */
function onOwnRowCleared(): void {
  selectedOwnDefName.value = null;
  isCreatingOwnRow.value = false;
}

const rowEditorContainerRef = useTemplateRef<HTMLDivElement>("rowEditorContainer");
useAssignmentEditorKeys({
  rows: () => coverage.value?.rows ?? [],
  selectedTarget: () => selectedTarget.value,
  onSelect: selectRow,
  editorContainer: () => rowEditorContainerRef.value,
});

const { mutateAsync: deleteAssignment } = useDeleteAssignmentMutation();
const deleteDialogVisible = ref(false);

async function confirmDelete(): Promise<void> {
  await deleteAssignment(assignmentId.value);
  deleteDialogVisible.value = false;
  await router.push({ name: "assignments" });
}

/** `EditReferencesTargetsDialog` is the sole UI caller of `useUpdateAssignmentMutation`. */
const editRefsTargetsDialogVisible = ref(false);

// --- Add section ------------------------------------------------------

const addSectionDialogVisible = ref(false);
const { mutateAsync: listCandidates, isLoading: isListingCandidates } =
  useListAssignmentCandidatesMutation();
const { mutateAsync: addSection, isLoading: isAddingSection } = useAddAssignmentSectionMutation();
const addableCandidates = ref<CandidateSummaryDto[]>([]);
const listCandidatesError = ref<CommandErrorDescriptor | null>(null);
const selectedCandidateType = ref<string | null>(null);
const addSectionError = ref<CommandErrorDescriptor | null>(null);

async function openAddSectionDialog(): Promise<void> {
  const current = assignment.value;
  if (!current) {
    return;
  }
  addSectionDialogVisible.value = true;
  listCandidatesError.value = null;
  addSectionError.value = null;
  selectedCandidateType.value = null;
  addableCandidates.value = [];
  try {
    const existing = new Set(sections.value.map((section) => section.defType));
    const response = await listCandidates({
      refs: current.refs.map((member) => member.modId),
      excludedRefs: current.excludedRefs.map((member) => member.modId),
      targets: current.targets.map((member) => member.modId),
    });
    addableCandidates.value = response.candidates.filter(
      (candidate) => !existing.has(candidate.defType),
    );
  } catch (err) {
    listCandidatesError.value = describeCommandError(err);
  }
}

async function confirmAddSection(): Promise<void> {
  const current = assignment.value;
  const defType = selectedCandidateType.value;
  if (!current || !defType) {
    return;
  }
  addSectionError.value = null;
  try {
    await addSection({ assignmentId: current.id, defType });
    addSectionDialogVisible.value = false;
    selectedSectionType.value = defType;
  } catch (err) {
    addSectionError.value = describeCommandError(err);
  }
}

// --- Remove section -----------------------------------------------------

const removeSectionDialogVisible = ref(false);
const removeSectionDefType = ref<string | null>(null);
const { mutateAsync: removeSection, isLoading: isRemovingSection } =
  useRemoveAssignmentSectionMutation();
const sectionInUseReferences = ref<SectionReferenceDto[] | null>(null);
const removeSectionError = ref<CommandErrorDescriptor | null>(null);

function openRemoveSectionDialog(): void {
  if (!selectedSectionType.value) {
    return;
  }
  removeSectionDefType.value = selectedSectionType.value;
  removeSectionDialogVisible.value = true;
  sectionInUseReferences.value = null;
  removeSectionError.value = null;
}

async function confirmRemoveSection(force: boolean): Promise<void> {
  const current = assignment.value;
  const defType = removeSectionDefType.value;
  if (!current || !defType) {
    return;
  }
  removeSectionError.value = null;
  try {
    await removeSection({ assignmentId: current.id, defType, force });
    removeSectionDialogVisible.value = false;
    sectionInUseReferences.value = null;
  } catch (err) {
    if (
      err instanceof RimmergeError &&
      err.detail !== null &&
      err.detail.kind === "assignmentSectionInUse"
    ) {
      sectionInUseReferences.value = err.detail.referencedBy;
    } else {
      removeSectionError.value = describeCommandError(err);
    }
  }
}
</script>

<template>
  <div class="mx-auto flex max-w-7xl flex-col gap-6 p-6">
    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="assignment-detail-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="assignment-detail-error"
    >
      {{ error instanceof Error ? error.message : t("assignments.editor.loadFailed") }}
    </p>
    <template v-else-if="assignment">
      <header
        class="flex items-start justify-between gap-4"
        data-testid="assignment-detail-header"
      >
        <div>
          <h1 class="text-text text-xl font-semibold">
            {{ assignment.displayName }}
          </h1>
          <p class="text-text-muted font-mono text-sm">
            {{ assignment.packageId }}
          </p>
        </div>
        <div class="flex gap-2">
          <Button
            :label="t('assignments.editor.editRefsTargetsButton')"
            size="small"
            severity="secondary"
            outlined
            data-testid="open-edit-refs-targets-button"
            @click="editRefsTargetsDialogVisible = true"
          />
          <Button
            :label="t('assignments.editor.deleteButton')"
            size="small"
            severity="danger"
            outlined
            data-testid="delete-assignment-button"
            @click="deleteDialogVisible = true"
          />
        </div>
      </header>

      <div class="flex items-center justify-between gap-4">
        <Tabs
          v-if="sections.length > 0"
          :value="selectedSectionType ?? ''"
          @update:value="(value: string | number) => (selectedSectionType = String(value))"
        >
          <TabList>
            <Tab
              v-for="section in sections"
              :key="section.defType"
              :value="section.defType"
              :data-testid="`section-tab-${section.defType}`"
            >
              {{ section.defType }}
            </Tab>
          </TabList>
        </Tabs>
        <p
          v-else
          class="text-text-faint text-sm"
        >
          {{ t("assignments.editor.noSections") }}
        </p>
        <div class="flex gap-2">
          <Button
            :label="t('assignments.editor.addSectionButton')"
            size="small"
            severity="secondary"
            data-testid="open-add-section-button"
            @click="openAddSectionDialog"
          />
          <Button
            v-if="selectedSectionType"
            :label="t('assignments.editor.removeSectionButton')"
            size="small"
            severity="danger"
            outlined
            data-testid="open-remove-section-button"
            @click="openRemoveSectionDialog"
          />
        </div>
      </div>

      <template v-if="selectedSection && !selectedSection.isStandalone">
        <div class="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-6">
          <CoverageList
            :rows="coverage?.rows ?? []"
            :selected-target="selectedTarget"
            @select="selectRow"
          />
          <div ref="rowEditorContainer">
            <RowEditor
              v-if="selectedRow"
              :assignment="assignment"
              :coverage-row="selectedRow"
              :section-type="selectedSection.defType"
            />
            <p
              v-else
              class="text-text-faint text-sm"
            >
              {{ t("assignments.editor.noTargetSelected") }}
            </p>
          </div>
        </div>
      </template>
      <section
        v-else-if="selectedSection"
        class="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-6"
        data-testid="standalone-section-rows"
      >
        <div class="surface-card flex flex-col gap-2 p-4">
          <div class="flex items-center justify-between gap-2">
            <h2 class="text-text text-sm font-semibold">
              {{ t("assignments.editor.ownRowsHeading") }}
            </h2>
            <Button
              :label="t('assignments.editor.newRowButton')"
              size="small"
              severity="secondary"
              data-testid="standalone-new-row-button"
              @click="startNewOwnRow"
            />
          </div>
          <p class="text-text-faint text-xs">
            {{ t("assignments.editor.standaloneHint") }}
          </p>
          <p
            v-if="selectedSection.standaloneRows.length === 0 && !isCreatingOwnRow"
            class="text-text-faint text-sm"
            data-testid="standalone-section-empty"
          >
            {{ t("assignments.editor.noRowsYet") }}
          </p>
          <ul
            v-if="selectedSection.standaloneRows.length > 0"
            class="flex flex-col gap-0.5"
          >
            <li
              v-for="entry in selectedSection.standaloneRows"
              :key="entry.row.defName"
            >
              <button
                type="button"
                class="hover:bg-surface-2 flex w-full cursor-pointer flex-col gap-0.5 rounded px-2 py-1.5 text-left text-xs"
                :class="{ 'bg-accent-soft': entry.row.defName === selectedOwnDefName }"
                :aria-current="entry.row.defName === selectedOwnDefName"
                :data-testid="`standalone-row-${entry.row.defName}`"
                @click="selectOwnRow(entry.row.defName)"
              >
                <span class="font-mono font-medium">{{ entry.row.defName }}</span>
                <span class="text-text-faint">{{ entry.row.note ?? "—" }}</span>
              </button>
            </li>
          </ul>
        </div>
        <div ref="rowEditorContainer">
          <RowEditor
            v-if="isCreatingOwnRow || selectedOwnDefName"
            :assignment="assignment"
            :section-type="selectedSection.defType"
            :own-def-name="isCreatingOwnRow ? null : selectedOwnDefName"
            @own-row-saved="onOwnRowSaved"
            @own-row-cleared="onOwnRowCleared"
          />
          <p
            v-else
            class="text-text-faint text-sm"
          >
            {{ t("assignments.editor.selectOrNewRow") }}
          </p>
        </div>
      </section>

      <section class="surface-card p-4">
        <h2 class="text-text mb-2 text-sm font-semibold">
          {{ t("assignments.editor.exportHeading") }}
        </h2>
        <AssignmentExportPanel
          :assignment-id="assignment.id"
          :export-dir="assignment.exportDir"
        />
      </section>
    </template>

    <Dialog
      v-model:visible="deleteDialogVisible"
      modal
      :header="t('assignments.editor.deleteDialogHeader')"
      data-testid="delete-assignment-dialog"
    >
      <p class="text-sm">
        {{ t("assignments.editor.deleteDialogBody") }}
      </p>
      <div class="mt-4 flex justify-end gap-2">
        <Button
          :label="t('common.cancel')"
          size="small"
          severity="secondary"
          data-testid="delete-assignment-cancel"
          @click="deleteDialogVisible = false"
        />
        <Button
          :label="t('assignments.editor.deleteButton')"
          size="small"
          severity="danger"
          data-testid="delete-assignment-confirm"
          @click="confirmDelete"
        />
      </div>
    </Dialog>

    <EditReferencesTargetsDialog
      v-if="assignment"
      :assignment="assignment"
      :visible="editRefsTargetsDialogVisible"
      @update:visible="(value) => (editRefsTargetsDialogVisible = value)"
    />

    <Dialog
      v-model:visible="addSectionDialogVisible"
      modal
      :header="t('assignments.editor.addSectionDialogHeader')"
      data-testid="add-section-dialog"
    >
      <p
        v-if="isListingCandidates"
        class="text-text-muted text-sm"
      >
        {{ t("assignments.editor.listingCandidates") }}
      </p>
      <Message
        v-else-if="listCandidatesError"
        severity="error"
        data-testid="add-section-list-error"
      >
        <div class="font-medium">
          {{ tm(listCandidatesError.title) }}
        </div>
        <div>{{ tm(listCandidatesError.detail) }}</div>
        <div
          v-if="listCandidatesError.technicalDetail"
          class="text-xs opacity-75"
        >
          {{ listCandidatesError.technicalDetail }}
        </div>
      </Message>
      <template v-else-if="addableCandidates.length > 0">
        <div
          class="flex flex-col gap-2"
          role="radiogroup"
          :aria-label="t('assignments.editor.candidateGroupLabel')"
        >
          <label
            v-for="candidate in addableCandidates"
            :key="candidate.defType"
            class="flex items-center gap-2 text-sm"
          >
            <RadioButton
              :model-value="selectedCandidateType"
              :value="candidate.defType"
              :data-testid="`add-section-candidate-${candidate.defType}`"
              @update:model-value="(value: string) => (selectedCandidateType = value)"
            />
            {{
              t(
                "assignments.wizard.instanceCount",
                { defType: candidate.defType, count: candidate.instanceCount },
                candidate.instanceCount,
              )
            }}
          </label>
        </div>
      </template>
      <p
        v-else
        class="text-text-faint text-sm"
        data-testid="add-section-empty"
      >
        {{ t("assignments.editor.noAddableCandidates") }}
      </p>
      <Message
        v-if="addSectionError"
        severity="error"
        data-testid="add-section-error"
      >
        <div class="font-medium">
          {{ tm(addSectionError.title) }}
        </div>
        <div>{{ tm(addSectionError.detail) }}</div>
        <div
          v-if="addSectionError.technicalDetail"
          class="text-xs opacity-75"
        >
          {{ addSectionError.technicalDetail }}
        </div>
      </Message>
      <div class="mt-4 flex justify-end gap-2">
        <Button
          :label="t('common.cancel')"
          size="small"
          severity="secondary"
          data-testid="add-section-cancel"
          @click="addSectionDialogVisible = false"
        />
        <Button
          :label="t('assignments.editor.addButton')"
          size="small"
          :disabled="!selectedCandidateType"
          :loading="isAddingSection"
          data-testid="add-section-confirm"
          @click="confirmAddSection"
        />
      </div>
    </Dialog>

    <Dialog
      v-model:visible="removeSectionDialogVisible"
      modal
      :header="t('assignments.editor.removeSectionDialogHeader')"
      data-testid="remove-section-dialog"
    >
      <p class="text-sm">
        {{ t("assignments.editor.removeSectionBody", { defType: removeSectionDefType }) }}
      </p>
      <template v-if="sectionInUseReferences && sectionInUseReferences.length > 0">
        <Message
          severity="warn"
          data-testid="remove-section-in-use"
        >
          {{
            t(
              "assignments.editor.sectionInUse",
              { count: sectionInUseReferences.length },
              sectionInUseReferences.length,
            )
          }}
        </Message>
        <ul class="text-text-muted mt-2 list-inside list-disc text-xs">
          <li
            v-for="(reference, index) in sectionInUseReferences"
            :key="index"
            :data-testid="`remove-section-reference-${index}`"
          >
            {{
              t("assignments.editor.referenceLine", {
                defType: reference.defType,
                row: reference.row,
                path: reference.path,
              })
            }}
          </li>
        </ul>
      </template>
      <Message
        v-if="removeSectionError"
        severity="error"
        data-testid="remove-section-error"
      >
        <div class="font-medium">
          {{ tm(removeSectionError.title) }}
        </div>
        <div>{{ tm(removeSectionError.detail) }}</div>
        <div
          v-if="removeSectionError.technicalDetail"
          class="text-xs opacity-75"
        >
          {{ removeSectionError.technicalDetail }}
        </div>
      </Message>
      <div class="mt-4 flex justify-end gap-2">
        <Button
          :label="t('common.cancel')"
          size="small"
          severity="secondary"
          data-testid="remove-section-cancel"
          @click="removeSectionDialogVisible = false"
        />
        <Button
          v-if="sectionInUseReferences && sectionInUseReferences.length > 0"
          :label="t('assignments.editor.removeAnywayButton')"
          size="small"
          severity="danger"
          :loading="isRemovingSection"
          data-testid="remove-section-force"
          @click="confirmRemoveSection(true)"
        />
        <Button
          v-else
          :label="t('assignments.editor.removeButton')"
          size="small"
          severity="danger"
          :loading="isRemovingSection"
          data-testid="remove-section-confirm"
          @click="confirmRemoveSection(false)"
        />
      </div>
    </Dialog>
  </div>
</template>
