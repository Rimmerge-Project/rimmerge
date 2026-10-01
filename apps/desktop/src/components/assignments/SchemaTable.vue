<script setup lang="ts">
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { asRecord } from "@/types/dtoMaps";
import type { AssignmentSchemaDto } from "@/types/generated/AssignmentSchemaDto";
import type { FieldRoleDto } from "@/types/generated/FieldRoleDto";
import type { FieldSpecDto } from "@/types/generated/FieldSpecDto";
import type { UnreadableTargetDto } from "@/types/generated/UnreadableTargetDto";
import { cardinalityLabel } from "@/utils/cardinality";

/**
 * The wizard's inferred-schema table: one row per field, a role dropdown
 * to reclassify it, and an "add field" form for one the XML never showed.
 * Mutates a local draft
 * `AssignmentSchemaDto` (v-model) entirely client-side, mirroring
 * `rim_resolve::domain::AssignmentSchema::confirm_role`/`add_field`'s own
 * pure logic (kept the original inferred role across repeated
 * reclassification; a duplicate path is rejected) — no round trip to the
 * backend happens until the wizard's own "Create" step, per this DTO
 * layer's own documented deviation (schema edits are wizard-only).
 */
const { schema, unreadableTargets = [] } = defineProps<{
  schema: AssignmentSchemaDto;
  unreadableTargets?: UnreadableTargetDto[];
}>();
const emit = defineEmits<{ "update:schema": [schema: AssignmentSchemaDto] }>();
const { t } = useI18n();
const tm = useTranslateMessage();

/** The role kinds this table's own dropdown offers — `Chances` is never manually assigned here (it only ever comes from inference pairing a float list to its sibling slot). */
const ROLE_KINDS = ["targetKey", "itemSlot", "scalar", "opaque"] as const;
type RoleKind = (typeof ROLE_KINDS)[number];

/** A literal `en.json` key per {@link FieldRoleDto} type — `roleOptions`/the "Chances" option below resolve each through `t()` reactively, so a locale switch updates the labels. */
const ROLE_LABEL_KEYS: Record<FieldRoleDto["type"], string> = {
  targetKey: "fieldRole.targetKey",
  itemSlot: "fieldRole.itemSlot",
  chances: "fieldRole.chances",
  scalar: "fieldRole.scalar",
  opaque: "fieldRole.opaque",
};

/** `schema.fields`, read as a plain `Record` — see `types/dtoMaps.ts`'s own doc comment. */
const fields = computed(() => asRecord(schema.fields));

function roleDefType(role: FieldRoleDto): string | null {
  return role.type === "targetKey" || role.type === "itemSlot" ? role.defType : null;
}

/** Builds a fresh role of `kind`, reusing `current`'s own `defType` when both kinds carry one. */
function buildRole(kind: RoleKind, current: FieldRoleDto): FieldRoleDto {
  const defType = roleDefType(current) ?? "";
  switch (kind) {
    case "targetKey":
      return { type: "targetKey", defType };
    case "itemSlot":
      return { type: "itemSlot", defType };
    case "scalar":
      return { type: "scalar", kind: { type: "text" }, default: null };
    case "opaque":
      return { type: "opaque" };
  }
}

/** Structural equality over `FieldRoleDto` — the two variants this table ever builds (`targetKey`/`itemSlot`'s `defType`, `scalar`'s `kind`/`default`) are the only ones whose fields can actually differ between two roles of the same `type`. Each `a.type === "x" && b.type === "x"` guard narrows both sides directly (no `as typeof a` cast needed) — TypeScript's discriminated-union narrowing already covers a two-operand check like this. */
function roleEquals(a: FieldRoleDto, b: FieldRoleDto): boolean {
  if (a.type === "targetKey" && b.type === "targetKey") {
    return a.defType === b.defType;
  }
  if (a.type === "itemSlot" && b.type === "itemSlot") {
    return a.defType === b.defType;
  }
  if (a.type === "chances" && b.type === "chances") {
    return a.forSlot === b.forSlot;
  }
  if (a.type === "scalar" && b.type === "scalar") {
    return a.default === b.default && a.kind.type === b.kind.type;
  }
  return a.type === b.type;
}

/**
 * Reclassifies one field's role, keeping the *original* inferred role
 * across repeated changes — mirrors `confirm_role`'s own rule exactly,
 * guard included: the original role is only ever captured the *first*
 * time the role genuinely changes (`role != spec.role`), never on a
 * reclassification that picks the same kind/def type the field already
 * had (a no-op re-selection would otherwise stomp `inferredRole` from
 * `null` to the current role even though nothing changed).
 */
function reclassify(path: string, kind: RoleKind): void {
  const spec = fields.value[path];
  if (!spec) {
    return;
  }
  const role = buildRole(kind, spec.role);
  const roleChanged = !roleEquals(role, spec.role);
  const inferredRole =
    spec.inferredRole !== null ? spec.inferredRole : roleChanged ? spec.role : null;
  emit("update:schema", {
    ...schema,
    fields: { ...fields.value, [path]: { ...spec, role, inferredRole } },
  });
}

/** Sets a `targetKey`/`itemSlot` field's own `defType` text. */
function setDefType(path: string, defType: string): void {
  const spec = fields.value[path];
  if (!spec || (spec.role.type !== "targetKey" && spec.role.type !== "itemSlot")) {
    return;
  }
  const role: FieldRoleDto = { type: spec.role.type, defType };
  emit("update:schema", { ...schema, fields: { ...fields.value, [path]: { ...spec, role } } });
}

function removeField(path: string): void {
  const updated = { ...fields.value };
  delete updated[path];
  emit("update:schema", { ...schema, fields: updated });
}

const newFieldPath = ref("");
const newFieldKind = ref<RoleKind>("scalar");
const newFieldDefType = ref("");
const addFieldError = ref<string | null>(null);

function addField(): void {
  addFieldError.value = null;
  const path = newFieldPath.value.trim();
  if (path.length === 0) {
    addFieldError.value = t("assignments.schemaTable.pathRequired");
    return;
  }
  if (path in fields.value) {
    addFieldError.value = t("assignments.schemaTable.pathAlreadyExists", { path });
    return;
  }
  const isList = newFieldKind.value === "targetKey" || newFieldKind.value === "itemSlot";
  const role = buildRole(newFieldKind.value, { type: "opaque" });
  const withDefType: FieldRoleDto =
    role.type === "targetKey" || role.type === "itemSlot"
      ? { ...role, defType: newFieldDefType.value.trim() }
      : role;
  const spec: FieldSpecDto = {
    role: withDefType,
    cardinality: isList ? "list" : "scalar",
    observed: [0, 0],
    inferredRole: null,
  };
  emit("update:schema", { ...schema, fields: { ...fields.value, [path]: spec } });
  newFieldPath.value = "";
  newFieldDefType.value = "";
}

const roleOptions = computed(() =>
  ROLE_KINDS.map((kind) => ({ label: t(ROLE_LABEL_KEYS[kind]), value: kind })),
);

const fieldRows = computed(() =>
  Object.entries(fields.value)
    .map(([path, spec]) => ({ path, spec }))
    .sort((a, b) => a.path.localeCompare(b.path)),
);
</script>

<template>
  <div
    class="flex flex-col gap-3"
    data-testid="schema-table"
  >
    <div class="overflow-x-auto">
      <table class="w-full text-left text-sm">
        <thead>
          <tr class="text-text-muted text-xs uppercase">
            <th class="py-1 pr-2">
              {{ t("assignments.schemaTable.fieldHeader") }}
            </th>
            <th class="py-1 pr-2">
              {{ t("assignments.schemaTable.roleHeader") }}
            </th>
            <th class="py-1 pr-2">
              {{ t("assignments.schemaTable.typeHeader") }}
            </th>
            <th class="py-1 pr-2">
              {{ t("assignments.schemaTable.cardinalityHeader") }}
            </th>
            <th class="py-1 pr-2">
              {{ t("assignments.schemaTable.observedHeader") }}
            </th>
            <th class="py-1" />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="row in fieldRows"
            :key="row.path"
            class="border-border-subtle border-t"
            :data-testid="`schema-row-${row.path}`"
          >
            <td class="py-1.5 pr-2 font-mono text-xs">
              {{ row.path }}
            </td>
            <td class="py-1.5 pr-2">
              <Select
                :model-value="row.spec.role.type === 'chances' ? 'chances' : row.spec.role.type"
                :options="
                  row.spec.role.type === 'chances'
                    ? [{ label: t('fieldRole.chances'), value: 'chances' }]
                    : roleOptions
                "
                option-label="label"
                option-value="value"
                :disabled="row.spec.role.type === 'chances'"
                size="small"
                :data-testid="`schema-role-select-${row.path}`"
                @update:model-value="(kind: RoleKind) => reclassify(row.path, kind)"
              />
            </td>
            <td class="py-1.5 pr-2">
              <InputText
                v-if="row.spec.role.type === 'targetKey' || row.spec.role.type === 'itemSlot'"
                :model-value="roleDefType(row.spec.role) ?? ''"
                size="small"
                :placeholder="t('assignments.schemaTable.defTypePlaceholder')"
                :data-testid="`schema-deftype-input-${row.path}`"
                @update:model-value="(value: string | undefined) => setDefType(row.path, value ?? '')"
              />
              <span
                v-else
                class="text-text-faint text-xs"
              >—</span>
            </td>
            <td class="py-1.5 pr-2 text-xs">
              {{ tm(cardinalityLabel(row.spec.cardinality)) }}
            </td>
            <td class="py-1.5 pr-2 text-xs">
              {{ row.spec.observed[0] }}/{{ row.spec.observed[1] }}
            </td>
            <td class="py-1.5">
              <button
                type="button"
                class="text-text-faint hover:text-status-danger cursor-pointer text-xs"
                :data-testid="`schema-remove-${row.path}`"
                @click="removeField(row.path)"
              >
                {{ t("assignments.schemaTable.hideButton") }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div
      v-if="unreadableTargets.length > 0"
      class="bg-status-input-soft text-status-input rounded p-2 text-xs"
      data-testid="schema-unreadable-targets"
    >
      <p class="font-medium">
        {{
          t(
            "assignments.schemaTable.unreadableTargets",
            { count: unreadableTargets.length },
            unreadableTargets.length,
          )
        }}
      </p>
      <ul class="list-inside list-disc">
        <li
          v-for="target in unreadableTargets"
          :key="`${target.field}-${target.defName}`"
        >
          {{
            t("assignments.schemaTable.unreadableTargetLine", {
              field: target.field,
              defType: target.defType,
              defName: target.defName,
              reason: target.reason,
            })
          }}
        </li>
      </ul>
    </div>

    <div class="flex flex-wrap items-end gap-2">
      <label class="flex flex-col gap-1 text-xs">
        <span class="text-text-muted">{{ t("assignments.schemaTable.newFieldPathLabel") }}</span>
        <InputText
          v-model="newFieldPath"
          size="small"
          data-testid="schema-new-field-path"
        />
      </label>
      <label class="flex flex-col gap-1 text-xs">
        <span class="text-text-muted">{{ t("assignments.schemaTable.roleLabel") }}</span>
        <Select
          v-model="newFieldKind"
          :options="roleOptions"
          option-label="label"
          option-value="value"
          size="small"
          data-testid="schema-new-field-role"
        />
      </label>
      <label
        v-if="newFieldKind === 'targetKey' || newFieldKind === 'itemSlot'"
        class="flex flex-col gap-1 text-xs"
      >
        <span class="text-text-muted">{{ t("assignments.schemaTable.defTypeLabel") }}</span>
        <InputText
          v-model="newFieldDefType"
          size="small"
          data-testid="schema-new-field-deftype"
        />
      </label>
      <Button
        :label="t('assignments.schemaTable.addFieldButton')"
        size="small"
        severity="secondary"
        data-testid="schema-add-field-button"
        @click="addField"
      />
    </div>
    <p
      v-if="addFieldError"
      class="text-status-danger text-xs"
      data-testid="schema-add-field-error"
    >
      {{ addFieldError }}
    </p>
  </div>
</template>
