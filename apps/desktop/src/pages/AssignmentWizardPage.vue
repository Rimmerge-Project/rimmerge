<script setup lang="ts">
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import RadioButton from "primevue/radiobutton";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import SchemaTable from "@/components/assignments/SchemaTable.vue";
import ModPicker from "@/components/mods/ModPicker.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import {
  useCreateAssignmentMutation,
  useInferAssignmentCandidateMutation,
  useListAssignmentCandidatesMutation,
} from "@/queries/assignments";
import { asRecord } from "@/types/dtoMaps";
import type { AssignmentSchemaDto } from "@/types/generated/AssignmentSchemaDto";
import type { CandidateSummaryDto } from "@/types/generated/CandidateSummaryDto";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { UnreadableTargetDto } from "@/types/generated/UnreadableTargetDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The patch maker's creation wizard: name/identity, then R/T pickers, then a two-phase candidate step
 * — `list_assignment_candidates`' own cheap summary list (type, instance
 * count, owners), then `infer_assignment_candidate`'s real schema for
 * whichever one type the user picks — with an inferred-schema table to
 * confirm or reclassify, then create. One page with sequential sections
 * rather than a modal stepper — `PatchListPage`'s own inline create
 * form's shape, not a separate component per step.
 *
 * T may be left empty: a candidate with no `TargetKey` field at all (a
 * "new def" candidate, e.g. authoring fresh content rather than patching
 * an existing def) is only offered by `infer_assignment_candidate` when T
 * is empty, and such a project's own rows are free-standing instances
 * rather than target-addressed ones (`AssignmentEditorPage.vue`'s own
 * job once created) — see
 * `rim_resolve::domain::AssignmentProject::is_standalone`'s own doc
 * comment.
 */
const { t } = useI18n();
const tm = useTranslateMessage();
const router = useRouter();

const name = ref("");
const packageId = ref("");
const displayName = ref("");

const refs = ref<ModRefDto[]>([]);
const excludedRefIds = ref<Set<string>>(new Set());
const targets = ref<ModRefDto[]>([]);

const { mutateAsync: listCandidates, isLoading: isListing } = useListAssignmentCandidatesMutation();
const listError = ref<CommandErrorDescriptor | null>(null);
const effectiveRefs = ref<ModRefDto[]>([]);
const summaries = ref<CandidateSummaryDto[]>([]);
const selectedDefType = ref<string | null>(null);

const { mutateAsync: inferCandidate, isLoading: isInferring } =
  useInferAssignmentCandidateMutation();
const inferError = ref<CommandErrorDescriptor | null>(null);
/** The confirmed (possibly user-edited) schema for whichever candidate has been inferred so far, keyed by def type so switching candidates never loses an in-progress edit. */
const editedSchemas = ref<Record<string, AssignmentSchemaDto>>({});
const unreadableTargetsByType = ref<Record<string, UnreadableTargetDto[]>>({});
const excludedTargetsByType = ref<Record<string, number>>({});

const selectedSchema = computed<AssignmentSchemaDto | null>(() =>
  selectedDefType.value ? (editedSchemas.value[selectedDefType.value] ?? null) : null,
);
const selectedUnreadableTargets = computed<UnreadableTargetDto[]>(
  () => (selectedDefType.value && unreadableTargetsByType.value[selectedDefType.value]) || [],
);
const selectedExcludedTargetCount = computed<number>(
  () => (selectedDefType.value && excludedTargetsByType.value[selectedDefType.value]) || 0,
);
const isStandaloneSchema = computed(
  () => selectedSchema.value !== null && !hasTargetKeyField(selectedSchema.value),
);

function hasTargetKeyField(schema: AssignmentSchemaDto): boolean {
  return Object.values(asRecord(schema.fields)).some((spec) => spec?.role.type === "targetKey");
}

async function runList(): Promise<void> {
  listError.value = null;
  summaries.value = [];
  selectedDefType.value = null;
  try {
    const response = await listCandidates({
      refs: refs.value.map((member) => member.modId),
      excludedRefs: [...excludedRefIds.value],
      targets: targets.value.map((member) => member.modId),
    });
    effectiveRefs.value = response.effectiveRefs;
    summaries.value = response.candidates;
  } catch (err) {
    listError.value = describeCommandError(err);
  }
}

async function selectCandidate(defType: string): Promise<void> {
  selectedDefType.value = defType;
  inferError.value = null;
  if (editedSchemas.value[defType]) {
    return;
  }
  try {
    const candidate = await inferCandidate({
      refs: refs.value.map((member) => member.modId),
      excludedRefs: [...excludedRefIds.value],
      targets: targets.value.map((member) => member.modId),
      defType,
    });
    editedSchemas.value = { ...editedSchemas.value, [defType]: candidate.schema };
    unreadableTargetsByType.value = {
      ...unreadableTargetsByType.value,
      [defType]: candidate.unreadableTargets,
    };
    excludedTargetsByType.value = {
      ...excludedTargetsByType.value,
      [defType]: candidate.excludedTargets.length,
    };
  } catch (err) {
    inferError.value = describeCommandError(err);
    selectedDefType.value = null;
  }
}

function toggleExcluded(modId: string): void {
  const next = new Set(excludedRefIds.value);
  if (next.has(modId)) {
    next.delete(modId);
  } else {
    next.add(modId);
  }
  excludedRefIds.value = next;
}

function updateSchema(schema: AssignmentSchemaDto): void {
  if (!selectedDefType.value) {
    return;
  }
  editedSchemas.value = { ...editedSchemas.value, [selectedDefType.value]: schema };
}

const { mutateAsync: create, isLoading: isCreating } = useCreateAssignmentMutation();
const createError = ref<CommandErrorDescriptor | null>(null);

async function submitCreate(): Promise<void> {
  const schema = selectedSchema.value;
  if (!schema) {
    return;
  }
  createError.value = null;
  try {
    const created = await create({
      name: name.value,
      packageId: packageId.value,
      displayName: displayName.value,
      refs: refs.value.map((member) => member.modId),
      excludedRefs: [...excludedRefIds.value],
      targets: targets.value.map((member) => member.modId),
      schema,
    });
    await router.push({ name: "assignment-detail", params: { assignmentId: created.id } });
  } catch (err) {
    createError.value = describeCommandError(err);
  }
}

const canList = computed(() => refs.value.length > 0 && !isListing.value);
const canCreate = computed(
  () =>
    name.value.trim().length > 0 &&
    packageId.value.trim().length > 0 &&
    displayName.value.trim().length > 0 &&
    selectedSchema.value !== null &&
    (targets.value.length > 0 || isStandaloneSchema.value) &&
    !isCreating.value,
);
</script>

<template>
  <div class="mx-auto flex max-w-4xl flex-col gap-6 p-6">
    <h1 class="text-text text-xl font-semibold">
      {{ t("assignments.wizard.heading") }}
    </h1>

    <section class="surface-card flex flex-col gap-3 p-4">
      <h2 class="text-text text-sm font-semibold">
        {{ t("assignments.wizard.identityHeading") }}
      </h2>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("assignments.wizard.nameLabel") }}</span>
        <InputText
          v-model="name"
          data-testid="assignment-name-input"
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("assignments.wizard.packageIdLabel") }}</span>
        <InputText
          v-model="packageId"
          :placeholder="t('assignments.wizard.packageIdPlaceholder')"
          data-testid="assignment-package-id-input"
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("assignments.wizard.displayNameLabel") }}</span>
        <InputText
          v-model="displayName"
          data-testid="assignment-display-name-input"
        />
      </label>
    </section>

    <section class="surface-card flex flex-col gap-2 p-4">
      <h2 class="text-text text-sm font-semibold">
        {{ t("assignments.wizard.referencesHeading") }}
      </h2>
      <ModPicker
        :members="refs"
        :target-label="t('assignments.wizard.referencesHeading')"
        testid-prefix="refs"
        data-testid="refs-picker"
        @change="(members) => (refs = members)"
      />
    </section>

    <section class="surface-card flex flex-col gap-2 p-4">
      <h2 class="text-text text-sm font-semibold">
        {{ t("assignments.wizard.targetsHeading") }}
      </h2>
      <p class="text-text-faint text-xs">
        {{ t("assignments.wizard.targetsHint") }}
      </p>
      <ModPicker
        :members="targets"
        :target-label="t('assignments.wizard.targetsHeading')"
        testid-prefix="targets"
        data-testid="targets-picker"
        @change="(members) => (targets = members)"
      />
    </section>

    <Button
      :label="t('assignments.wizard.listCandidatesButton')"
      size="small"
      :disabled="!canList"
      :loading="isListing"
      data-testid="propose-button"
      @click="runList"
    />
    <Message
      v-if="listError"
      severity="error"
      data-testid="propose-error"
    >
      <div class="font-medium">
        {{ tm(listError.title) }}
      </div>
      <div>{{ tm(listError.detail) }}</div>
      <div
        v-if="listError.technicalDetail"
        class="text-xs opacity-75"
      >
        {{ listError.technicalDetail }}
      </div>
    </Message>

    <section
      v-if="effectiveRefs.length > 0"
      class="surface-card flex flex-col gap-2 p-4"
      data-testid="effective-refs"
    >
      <h2 class="text-text text-sm font-semibold">
        {{ t("assignments.wizard.effectiveRefsHeading") }}
      </h2>
      <ul class="flex flex-wrap gap-1">
        <li
          v-for="ref_ in effectiveRefs"
          :key="ref_.modId"
          class="bg-surface-2 flex items-center gap-1 rounded-full py-0.5 pr-1 pl-2 text-xs"
          :data-testid="`effective-ref-${ref_.modId}`"
        >
          {{ ref_.name }}
          <button
            type="button"
            class="text-text-faint hover:text-status-danger cursor-pointer"
            :class="{ 'text-status-input': excludedRefIds.has(ref_.modId) }"
            :title="
              excludedRefIds.has(ref_.modId)
                ? t('assignments.wizard.excludedTitle')
                : t('assignments.wizard.includeTitle')
            "
            :data-testid="`effective-ref-toggle-${ref_.modId}`"
            @click="toggleExcluded(ref_.modId)"
          >
            {{
              excludedRefIds.has(ref_.modId)
                ? t("assignments.wizard.excludedTag")
                : t("assignments.wizard.includedTag")
            }}
          </button>
        </li>
      </ul>
    </section>

    <section
      v-if="summaries.length > 0"
      class="surface-card flex flex-col gap-3 p-4"
      data-testid="candidates-section"
    >
      <h2 class="text-text text-sm font-semibold">
        {{ t("assignments.wizard.candidatesHeading") }}
      </h2>
      <div
        class="flex flex-wrap gap-3"
        role="radiogroup"
        :aria-label="t('assignments.wizard.candidateGroupLabel')"
      >
        <label
          v-for="summary in summaries"
          :key="summary.defType"
          class="flex items-center gap-2 text-sm"
        >
          <RadioButton
            :model-value="selectedDefType"
            :value="summary.defType"
            :data-testid="`candidate-radio-${summary.defType}`"
            @update:model-value="selectCandidate(summary.defType)"
          />
          {{
            t(
              "assignments.wizard.instanceCount",
              { defType: summary.defType, count: summary.instanceCount },
              summary.instanceCount,
            )
          }}
        </label>
      </div>
      <Message
        v-if="inferError"
        severity="error"
        data-testid="infer-error"
      >
        <div class="font-medium">
          {{ tm(inferError.title) }}
        </div>
        <div>{{ tm(inferError.detail) }}</div>
        <div
          v-if="inferError.technicalDetail"
          class="text-xs opacity-75"
        >
          {{ inferError.technicalDetail }}
        </div>
      </Message>

      <p
        v-if="selectedDefType && isInferring && !selectedSchema"
        class="text-text-faint text-xs"
      >
        {{ t("assignments.wizard.inferringSchema") }}
      </p>

      <p
        v-if="isStandaloneSchema"
        class="text-status-input text-xs"
        data-testid="standalone-note"
      >
        {{ t("assignments.wizard.standaloneNote") }}
      </p>
      <p
        v-if="selectedExcludedTargetCount > 0"
        class="text-text-faint text-xs"
        data-testid="excluded-targets-note"
      >
        {{
          t(
            "assignments.wizard.excludedTargetsNote",
            { count: selectedExcludedTargetCount },
            selectedExcludedTargetCount,
          )
        }}
      </p>

      <SchemaTable
        v-if="selectedSchema"
        :schema="selectedSchema"
        :unreadable-targets="selectedUnreadableTargets"
        @update:schema="updateSchema"
      />
    </section>

    <Message
      v-if="createError"
      severity="error"
      data-testid="create-error"
    >
      <div class="font-medium">
        {{ tm(createError.title) }}
      </div>
      <div>{{ tm(createError.detail) }}</div>
      <div
        v-if="createError.technicalDetail"
        class="text-xs opacity-75"
      >
        {{ createError.technicalDetail }}
      </div>
    </Message>

    <Button
      :label="t('assignments.wizard.createButton')"
      size="small"
      :disabled="!canCreate"
      :loading="isCreating"
      data-testid="assignment-create-button"
      @click="submitCreate"
    />
  </div>
</template>
