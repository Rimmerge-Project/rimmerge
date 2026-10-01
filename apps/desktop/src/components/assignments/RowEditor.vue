<script setup lang="ts">
import Button from "primevue/button";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import ToggleSwitch from "primevue/toggleswitch";
import { computed, provide, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink } from "vue-router";
import RowEditorMatches from "@/components/assignments/RowEditorMatches.vue";
import RowEditorSlots from "@/components/assignments/RowEditorSlots.vue";
import DefGraphicViewer from "@/components/graphics/DefGraphicViewer.vue";
import { ROW_EDITOR_KEY } from "@/composables/rowEditorContext";
import { useModLabel } from "@/composables/useModLabel";
import { useRowDescriptor } from "@/composables/useRowDescriptor";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import {
  useClearAssignmentRowMutation,
  useCopyAssignmentRowFromMutation,
  useSetAssignmentRowMutation,
} from "@/queries/assignments";
import { defRefOf } from "@/types/brands";
import { asRecord } from "@/types/dtoMaps";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentRowDto } from "@/types/generated/AssignmentRowDto";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";
import type { DroppedItemSlotValueDto } from "@/types/generated/DroppedItemSlotValueDto";
import type { FieldSpecDto } from "@/types/generated/FieldSpecDto";
import type { RowValueDto } from "@/types/generated/RowValueDto";
import type { ScalarKindDto } from "@/types/generated/ScalarKindDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The editor's centre pane: one target's own row — an "override" badge
 * when the target already has a match, a picker per item slot, chance
 * inputs paired to their slot, scalar inputs with the schema default
 * pre-filled, and "copy from" seeded from this target's own existing
 * matches (see this component's own module doc comment below for why
 * that's narrower than "any existing instance").
 *
 * **Prefill from the winning instance**: opening a target that has no
 * row in this project yet, but does have a known `Winner::Existing`
 * under `coverageRow.winner`, silently runs the same
 * `copy_assignment_row_from` call the "Copy from" chips below trigger by
 * hand — never when this project already has a row for the target
 * (`existingRow`), so an already-authored row is never clobbered. Every
 * field a prefill (auto or manual) actually populated is tracked in
 * `prefillSource` (path -> source mod id) and rendered as a small "from
 * <mod>" label next to that field — a visible, per-field proof, not a
 * side note — and cleared the moment the user edits that field (it's
 * authored now, not borrowed).
 *
 * **Free-standing rows**: a free-standing
 * section's own row has no target to match through, so it's addressed by
 * its own `defName` instead — pass `ownDefName` (a real `defName` to edit
 * an existing row, or `null` to start a brand new one) and omit
 * `coverageRow` entirely. Everything target-only (the coverage queue's
 * own "matches"/"winner"/auto-prefill, the override badge) simply doesn't
 * apply and renders nothing; every other control (item pickers, chance
 * inputs, scalar controls, save/clear) is identical either way — see
 * {@link descriptor}, the one place the two modes are told apart.
 */
const {
  assignment,
  coverageRow = undefined,
  sectionType,
  ownDefName = undefined,
} = defineProps<{
  assignment: AssignmentDetailDto;
  /** Omit (together with giving `ownDefName`) to edit a free-standing row instead. */
  coverageRow?: CoverageRowDto;
  /** The selected tab's own section def type. */
  sectionType: string;
  /** Set (to a defName, or `null` for a brand-new row) to edit a free-standing section's own row instead of a target-keyed one. Omit entirely for the original target-keyed behavior via `coverageRow`. */
  ownDefName?: string | null;
}>();

const emit = defineEmits<{
  /** A free-standing row was saved under this `defName` — the parent should select it (switching this editor from "new row" to "editing X"). Never emitted for a target-keyed save. */
  ownRowSaved: [defName: string];
  /** A free-standing row was cleared — the parent should deselect it (this editor has nothing left to edit under its old `ownDefName`). Never emitted for a target-keyed clear. */
  ownRowCleared: [];
}>();

const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

const section = computed(() => assignment.sections.find((entry) => entry.defType === sectionType));

/** `section.schema.fields`, read as a plain `Record` — see `types/dtoMaps.ts`'s own doc comment. Declared ahead of `resetDraft`/`applyBorrowedRow` (below) because the scalar-slot clamp needs `itemSlotFields` and both run from a `watch({ immediate: true })` that fires during this component's own setup. */
const schemaFields = computed(() => asRecord(section.value?.schema.fields ?? {}));
const itemSlotFields = computed(() =>
  Object.entries(schemaFields.value).filter(([, spec]) => spec.role.type === "itemSlot"),
);
const chanceFields = computed(() =>
  Object.entries(schemaFields.value).filter(([, spec]) => spec.role.type === "chances"),
);
const scalarFields = computed(() =>
  Object.entries(schemaFields.value).filter(([, spec]) => spec.role.type === "scalar"),
);

const descriptor = useRowDescriptor(() => ({ assignment, coverageRow, ownDefName }), section, t);

/** Kept as its own name (rather than `descriptor.value.existing` inline everywhere) purely because the template/save-clear logic below already read `existingRow` before this refactor — same value, same meaning, both modes. */
const existingRow = computed(() => descriptor.value.existing);

const values = ref<Record<string, RowValueDto>>({});
const defName = ref(descriptor.value.defaultDefName);
const note = ref<string | null>(null);

/**
 * The "Inspect def" link's own def ref text: the target's, for a
 * target-keyed row (always known); the section's own def type paired
 * with whatever `defName` is currently typed, for a free-standing one —
 * `null` while that's still blank, since there's nothing to inspect yet
 * (a brand-new, unsaved row).
 */
const inspectDefRef = computed<string | null>(() => {
  const current = descriptor.value;
  if (current.type === "target") {
    return current.title;
  }
  return defName.value.length > 0 ? `${sectionType}/${defName.value}` : null;
});
const saveError = ref<CommandErrorDescriptor | null>(null);
/** Path -> source mod id, for every field a prefill (auto or "Copy from") populated and the user hasn't since edited. */
const prefillSource = ref<Record<string, string>>({});
/**
 * Path -> every name a `Cardinality::Scalar` `ItemSlot` value carried
 * before {@link clampScalarItemSlots} reduced it to one. A stored
 * row from before this constraint existed, or a copy-from/auto-prefill
 * source instance, can still carry more than one name — `render_leaf`'s
 * own scalar-slot rule silently drops such a field as a `SkippedField` at export
 * rather than keeping the first name, so the read/display path has to
 * make the same reduction visible rather than let it vanish unremarked.
 */
const clampedFrom = ref<Record<string, string[]>>({});
/**
 * Path -> every `ItemSlot` value a copy-from call (manual or auto-prefill)
 * had to drop because the def it named isn't active
 * (`CopyFromOutcome::dropped`'s own doc comment). Only ever populated by
 * {@link applyBorrowedRow} — a stored row read via {@link resetDraft}
 * never has anything here, since a value already accepted by
 * `set_assignment_row` is by definition not one of these.
 */
const droppedFromCopy = ref<Record<string, DroppedItemSlotValueDto[]>>({});
/**
 * Paths of a `ScalarKind::Number` field whose own *loaded* value was
 * already fractional despite an Int-looking `default` (`"13"`, say, next
 * to a stored `"0.35"` from a different source instance than the one the
 * default was inferred from) — captured once, in {@link
 * captureUnlockedFractionalFields}, whenever the draft is (re)loaded
 * (`resetDraft`/`applyBorrowedRow`), never recomputed live from the
 * field's *current* in-progress value: gating on the value the user is
 * still typing would create a feedback loop — the very first keystroke
 * that produces a fractional intermediate value (typing "7.9" into an
 * empty Int-looking field one digit at a time) would flip this field's
 * own entry on and permanently unlock decimals for the rest of the
 * session, defeating the Int constraint `looksIntegerLike` exists for in
 * the first place. Snapshotting at load time keeps a genuinely stored
 * fractional value from displaying truncated to `0` without that loop.
 */
const unlockedFractionalFields = ref<Set<string>>(new Set());

function captureUnlockedFractionalFields(raw: Record<string, RowValueDto>): void {
  const unlocked = new Set<string>();
  for (const [path, spec] of scalarFields.value) {
    if (spec.role.type !== "scalar" || spec.role.kind.type !== "number") {
      continue;
    }
    if (!looksIntegerLike(spec.role.default)) {
      continue;
    }
    const value = raw[path];
    const text = value?.kind === "text" ? value.text : "";
    const parsed = text.length > 0 ? Number(text) : null;
    if (parsed !== null && Number.isFinite(parsed) && !Number.isInteger(parsed)) {
      unlocked.add(path);
    }
  }
  unlockedFractionalFields.value = unlocked;
}

/**
 * True once any draft field has actually been edited by hand since the
 * last `resetDraft` — distinct from `prefillToken`: the token alone
 * only protects against the target moving on mid-fetch, not against a
 * user typing *into the current target's own draft* while a slower
 * `autoPrefill` fetch (a real install's XML read, `spawn_blocking`,
 * genuinely hundreds of ms) is still in flight for it. Read only by
 * `autoPrefill`'s own apply guard below; `copyFromMatch`'s manual "Copy
 * from" stays an unconditional overwrite regardless, since a user
 * clicking a chip is explicitly asking to replace what's there.
 */
const dirty = ref(false);

function clearPrefillSource(path: string): void {
  if (!(path in prefillSource.value)) {
    return;
  }
  const next = { ...prefillSource.value };
  delete next[path];
  prefillSource.value = next;
}

function clearClampedFrom(path: string): void {
  if (!(path in clampedFrom.value)) {
    return;
  }
  const next = { ...clampedFrom.value };
  delete next[path];
  clampedFrom.value = next;
}

function clearDroppedFromCopy(path: string): void {
  if (!(path in droppedFromCopy.value)) {
    return;
  }
  const next = { ...droppedFromCopy.value };
  delete next[path];
  droppedFromCopy.value = next;
}

/**
 * Reduces every `Cardinality::Scalar` `ItemSlot` value in `raw` down
 * to its first name, recording what got dropped in `clampedFrom` — run
 * on every value entering the draft from outside the user's own typing
 * (a stored row in `resetDraft`, a borrowed one in `applyBorrowedRow`),
 * never from `setNames` itself (a fresh user selection is already
 * cardinality-correct, `ItemPicker`'s own single-select radio mode saw
 * to that).
 */
function clampScalarItemSlots(raw: Record<string, RowValueDto>): Record<string, RowValueDto> {
  const clamped: Record<string, string[]> = {};
  const next = { ...raw };
  for (const [path, spec] of itemSlotFields.value) {
    if (spec.cardinality !== "scalar") {
      continue;
    }
    const value = next[path];
    const first = value?.kind === "names" ? value.names[0] : undefined;
    if (value?.kind !== "names" || value.names.length <= 1 || first === undefined) {
      continue;
    }
    clamped[path] = value.names;
    next[path] = { kind: "names", names: [first] };
  }
  clampedFrom.value = clamped;
  return next;
}

/**
 * Labels every non-`omit` field `sourceMod` filled — the whole row comes
 * from one source instance, so every populated field shares that same
 * source. `dropped` (`CopyAssignmentRowResultDto.dropped`) is grouped by
 * its own `path` into {@link droppedFromCopy} — a borrowed value the
 * backend already refused to carry into the row at all, so there is
 * nothing in `values` to clamp; this is purely a "here's what got left
 * out and why" disclosure, the copy-from sibling of {@link
 * clampScalarItemSlots}'s own `clampedFrom`.
 */
function applyBorrowedRow(
  row: AssignmentRowDto,
  sourceMod: string,
  dropped: DroppedItemSlotValueDto[] = [],
): void {
  values.value = clampScalarItemSlots(asRecord(row.values));
  captureUnlockedFractionalFields(values.value);
  defName.value = row.defName;
  note.value = row.note;
  const labels: Record<string, string> = {};
  for (const [path, value] of Object.entries(values.value)) {
    if (value.kind !== "omit") {
      labels[path] = sourceMod;
    }
  }
  prefillSource.value = labels;
  const droppedByPath: Record<string, DroppedItemSlotValueDto[]> = {};
  for (const value of dropped) {
    const existing = droppedByPath[value.path];
    if (existing) {
      existing.push(value);
    } else {
      droppedByPath[value.path] = [value];
    }
  }
  droppedFromCopy.value = droppedByPath;
}

const { mutateAsync: copyFrom, isLoading: isCopying } = useCopyAssignmentRowFromMutation();

/** Bumped on every `resetDraft`/manual copy, so a stale in-flight `copy_assignment_row_from` response can never overwrite a newer draft. */
let prefillToken = 0;

/**
 * Runs `copy_assignment_row_from` automatically for an uncovered target
 * with a known winner — never when this project already has its own row
 * for the target (`resetDraft` only calls this in that case). `token` is
 * `prefillToken`'s value as of the call that started this fetch; if it no
 * longer matches by the time the fetch resolves, the target moved on and
 * the result is discarded. `!dirty.value` additionally refuses to
 * apply onto a draft the user has since started typing into for this
 * same target — the token alone doesn't catch that case, since the
 * target hasn't changed. Best-effort: a failed fetch just leaves the
 * draft empty (or whatever the user's already typed), since the manual
 * "Copy from" chips (and plain hand-entry) still work.
 */
async function autoPrefill(token: number): Promise<void> {
  const current = descriptor.value;
  if (current.type !== "target" || current.winner?.kind !== "existing") {
    return;
  }
  const { winner } = current;
  try {
    const result = await copyFrom({
      assignmentId: assignment.id,
      section: section.value?.defType ?? null,
      target: current.target,
      sourceDefName: winner.match.instanceDefName,
    });
    if (token === prefillToken && !dirty.value) {
      applyBorrowedRow(result.row, winner.match.owner, result.dropped);
    }
  } catch {
    // Convenience only — the user can still copy a match by hand or fill the row themselves.
  }
}

function resetDraft(): void {
  const row = existingRow.value;
  values.value = clampScalarItemSlots(row ? asRecord(row.values) : {});
  captureUnlockedFractionalFields(values.value);
  defName.value = row?.defName ?? descriptor.value.defaultDefName;
  note.value = row?.note ?? null;
  saveError.value = null;
  prefillSource.value = {};
  droppedFromCopy.value = {};
  dirty.value = false;
  prefillToken++;
  // Auto-prefill is a target-keyed-only convenience (it copies from the
  // coverage queue's own known winner) — a free-standing row has no
  // coverage/winner concept to prefill from at all.
  if (!row && descriptor.value.type === "target") {
    void autoPrefill(prefillToken);
  }
}

/**
 * Identity, not reference: `coverageRow` is a prop typically fed by a
 * `computed` over a Pinia Colada query (`AssignmentEditorPage.vue`'s
 * `selectedRow`), and Colada has no structural sharing — a background
 * refetch (default `staleTime` of 5s, `refetchOnWindowFocus`, or any
 * mutation elsewhere invalidating every query) hands back a *new but
 * deep-equal* `target` object on every resolve. Watching `() =>
 * coverageRow.target` directly fired on every such refetch (a
 * non-deep watcher sees the new reference), silently wiping an
 * in-progress draft and re-running `autoPrefill`. Watching
 * `descriptor.value.key` (a plain string either mode already builds fresh
 * per render) instead only changes when the *row being edited* itself
 * actually changes — the target moving on, or (own mode) the parent
 * switching `ownDefName` to a different row or back to `null` for "new".
 */
watch(() => descriptor.value.key, resetDraft, { immediate: true });

function namesFor(path: string): string[] {
  const value = values.value[path];
  return value?.kind === "names" ? value.names : [];
}
function setNames(path: string, names: string[]): void {
  dirty.value = true;
  clearPrefillSource(path);
  clearClampedFrom(path);
  clearDroppedFromCopy(path);
  // An empty selection ("Clear selection" in a scalar `ItemPicker") is an
  // omit signal, matching `setNumber`'s own pattern below — a stored
  // `{kind:"names",names:[]}` isn't "no value", it's a field `render_leaf`
  // refuses ("expected exactly one") instead of simply treating it as
  // absent.
  if (names.length === 0) {
    if (path in values.value) {
      const rest = { ...values.value };
      delete rest[path];
      values.value = rest;
    }
    return;
  }
  values.value = { ...values.value, [path]: { kind: "names", names } };
}
function numbersFor(path: string): number[] {
  const value = values.value[path];
  return value?.kind === "numbers" ? value.numbers : [];
}
/** The chance value at `index`, or `null` when unset — `InputNumber`'s own model-value shape. */
function chanceAt(path: string, index: number): number | null {
  const value = numbersFor(path)[index];
  return value === undefined ? null : value;
}
/**
 * A cleared or emptied box comes back `null` from `InputNumber`
 * (never `NaN`/an unparsed string — the control itself rejects
 * non-numeric and locale-mismatched keystrokes, the same as the scalar
 * `Number` fields), so it's normalized to `0`
 * rather than propagating a hole into the fixed-length array
 * `render_leaf` expects to line up 1:1 with the paired slot's names.
 */
function setChanceAt(path: string, index: number, value: number | null): void {
  dirty.value = true;
  const current = [...numbersFor(path)];
  current[index] = value ?? 0;
  values.value = { ...values.value, [path]: { kind: "numbers", numbers: current } };
  clearPrefillSource(path);
}
function textFor(path: string): string {
  const value = values.value[path];
  return value?.kind === "text" ? value.text : "";
}
function setText(path: string, text: string): void {
  dirty.value = true;
  values.value = { ...values.value, [path]: { kind: "text", text } };
  clearPrefillSource(path);
}
function setDefName(value: string): void {
  dirty.value = true;
  defName.value = value;
}

/** A `ScalarKind::Bool` field's `RowValue` is still literal `"true"`/`"false"` text — RimWorld's XML deserializer expects exactly that spelling — only the control changes. `defaultText` (the schema's own most-common-observed value) backs an untouched field's initial position; picking it never itself writes a value (still omitted from the row until the user actually flips the switch). */
function boolFor(path: string, defaultText: string | null): boolean {
  const value = values.value[path];
  const text = value?.kind === "text" ? value.text : (defaultText ?? "false");
  return text.toLowerCase() === "true";
}
function setBool(path: string, checked: boolean): void {
  setText(path, checked ? "true" : "false");
}

/**
 * `ScalarKind::Number` round-trips through
 * `RowValue::Text` (`render_leaf` renders a `Scalar` field's text
 * verbatim) — only the control validates numeric-only input instead of
 * accepting anything. `FieldRoleDto`'s `Scalar` role carries only a
 * single `default` sample, never a min/max or a distinct-value list the
 * way `Enum` does, so there is no signal here for a "sensible range" a
 * slider could honestly be built from; guessing one (e.g. 0-100) would be
 * actively misleading for a field whose real domain might be 0.0-1.0 or
 * -50..500. A plain, editable number input is therefore used for every
 * `Number` field rather than a slider. `looksIntegerLike` is a heuristic over
 * that same one sample (no `Int`/`Float` split exists in `ScalarKind`
 * itself, only `Number`): a whole-number-looking default disables
 * fractional digits so the control can't produce one; an absent or
 * fractional-looking default leaves decimals allowed, since there's no
 * evidence the field is integer-only. `locale="en-US"` is pinned in the
 * template (not left to the OS default): `InputNumber` otherwise parses
 * typed digits against the *host's* ICU locale, and on a locale that
 * uses `.` as a thousands separator, a literal `0.35` silently becomes
 * `35` — RimWorld's
 * XML always wants a period decimal regardless of the modder's own OS
 * locale, so parsing must be locale-independent too, not just
 * `setNumber`'s own `String(value)` (which is already locale-independent
 * on the way out).
 *
 * `looksIntegerLike` alone gates on the schema's *default* sample
 * only — a field defaulting to a whole number like `"13"` pins
 * `maxFractionDigits` to `0` even when the loaded value is genuinely
 * fractional (`"0.35"`, from a different source instance than the one
 * the default was inferred from), which then displays a truncated `0`
 * while the underlying draft still holds the real `0.35` — the display
 * would contradict what actually gets exported. The template's own
 * `max-fraction-digits` additionally checks {@link
 * unlockedFractionalFields} (populated once at load, see its own doc
 * comment for why *not* live) so a field already holding a fractional
 * value keeps showing it.
 */
function looksIntegerLike(defaultText: string | null): boolean {
  return defaultText !== null && /^-?\d+$/.test(defaultText);
}

/** `spec.role`'s own `kind.type`, narrowed here (once) rather than in the template — a `spec.role.type === "scalar"` guard doesn't carry across separate template attribute expressions. `null` for a non-scalar field, which the scalar-fields loop never actually passes. */
function scalarKindType(spec: FieldSpecDto): ScalarKindDto["type"] | null {
  return spec.role.type === "scalar" ? spec.role.kind.type : null;
}
/** `spec.role`'s own `default`, narrowed the same way as {@link scalarKindType}. */
function scalarDefault(spec: FieldSpecDto): string | null {
  return spec.role.type === "scalar" ? spec.role.default : null;
}
function numberFor(path: string): number | null {
  const value = values.value[path];
  if (value?.kind !== "text" || value.text.length === 0) {
    return null;
  }
  const parsed = Number(value.text);
  return Number.isFinite(parsed) ? parsed : null;
}
function setNumber(path: string, value: number | null): void {
  if (value === null || !Number.isFinite(value)) {
    dirty.value = true;
    if (path in values.value) {
      const rest = { ...values.value };
      delete rest[path];
      values.value = rest;
    }
    clearPrefillSource(path);
    return;
  }
  setText(path, String(value));
}

const { mutateAsync: setRow, isLoading: isSaving } = useSetAssignmentRowMutation();
const { mutateAsync: clearRow, isLoading: isClearing } = useClearAssignmentRowMutation();

async function save(): Promise<void> {
  saveError.value = null;
  const current = descriptor.value;
  const row: AssignmentRowDto = { values: values.value, defName: defName.value, note: note.value };
  try {
    await setRow({
      assignmentId: assignment.id,
      section: section.value?.defType ?? null,
      target: current.target,
      row,
    });
    if (current.type === "own") {
      emit("ownRowSaved", defName.value);
    }
  } catch (err) {
    saveError.value = describeCommandError(err);
  }
}

async function clear(): Promise<void> {
  const current = descriptor.value;
  await clearRow({
    assignmentId: assignment.id,
    section: section.value?.defType ?? null,
    target: current.target,
    defName: current.type === "own" ? defName.value : null,
  });
  if (current.type === "own") {
    emit("ownRowCleared");
    return;
  }
  resetDraft();
}

async function copyFromMatch(sourceDefName: string, owner: string): Promise<void> {
  const current = descriptor.value;
  if (current.type !== "target") {
    return; // Unreachable via the template — a free-standing row has no "Copy from" chips.
  }
  saveError.value = null;
  prefillToken++; // Supersede any auto-prefill still in flight for this same target.
  try {
    const result = await copyFrom({
      assignmentId: assignment.id,
      section: section.value?.defType ?? null,
      target: current.target,
      sourceDefName,
    });
    applyBorrowedRow(result.row, owner, result.dropped);
  } catch (err) {
    saveError.value = describeCommandError(err);
  }
}

provide(ROW_EDITOR_KEY, {
  descriptor,
  modLabel,
  copyFromMatch,
  isCopying,
  itemSlotFields,
  chanceFields,
  prefillSource,
  clampedFrom,
  droppedFromCopy,
  namesFor,
  setNames,
  chanceAt,
  setChanceAt,
});
</script>

<template>
  <div
    class="flex flex-col gap-4"
    data-testid="row-editor"
  >
    <header class="flex items-start gap-4">
      <DefGraphicViewer
        v-if="descriptor.type === 'target'"
        :def-ref="defRefOf(descriptor.target.def.defType, descriptor.target.def.defName)"
        :def-owner="coverageRow?.owner ?? null"
      />
      <div class="flex min-w-0 flex-1 items-start justify-between gap-2">
        <div>
          <h3 class="text-text text-sm font-semibold">
            {{ descriptor.title }}
          </h3>
          <span
            v-if="descriptor.isOverride"
            class="bg-status-input-soft text-status-input rounded-full px-2 py-0.5 text-xs"
            data-testid="row-editor-override-badge"
          >
            {{
              t(
                "assignments.rowEditor.overrideBadge",
                { count: descriptor.matches.length },
                descriptor.matches.length,
              )
            }}
          </span>
        </div>
        <RouterLink
          v-if="inspectDefRef"
          :to="{ name: 'def', params: { defRef: inspectDefRef } }"
          class="text-accent text-xs"
          data-testid="row-editor-inspect-link"
        >
          {{ t("assignments.rowEditor.inspectDefLink") }}
        </RouterLink>
      </div>
    </header>

    <RowEditorMatches />

    <label class="flex flex-col gap-1 text-sm">
      <span class="text-text-muted">{{ t("assignments.rowEditor.defNameLabel") }}</span>
      <InputText
        :model-value="defName"
        size="small"
        data-testid="row-editor-def-name-input"
        @update:model-value="(value: string | undefined) => setDefName(value ?? '')"
      />
    </label>


    <RowEditorSlots :assignment="assignment" />

    <label
      v-for="[path, spec] in scalarFields"
      :key="path"
      class="flex flex-col gap-1 text-sm"
    >
      <span class="text-text-muted text-xs">
        {{ path }}
        <span
          v-if="prefillSource[path]"
          class="text-status-input"
          :data-testid="`row-editor-prefill-${path}`"
        >
          {{ t("assignments.rowEditor.prefillFrom", { mod: modLabel.label(prefillSource[path]) }) }}
        </span>
      </span>
      <ToggleSwitch
        v-if="scalarKindType(spec) === 'bool'"
        :model-value="boolFor(path, scalarDefault(spec))"
        :data-testid="`row-editor-scalar-${path}`"
        @update:model-value="(checked: boolean) => setBool(path, checked)"
      />
      <InputNumber
        v-else-if="scalarKindType(spec) === 'number'"
        :model-value="numberFor(path)"
        size="small"
        locale="en-US"
        :use-grouping="false"
        :min-fraction-digits="0"
        :max-fraction-digits="looksIntegerLike(scalarDefault(spec)) && !unlockedFractionalFields.has(path) ? 0 : 6"
        :placeholder="scalarDefault(spec) ?? undefined"
        :data-testid="`row-editor-scalar-${path}`"
        @update:model-value="(value: number | null) => setNumber(path, value)"
      />
      <InputText
        v-else
        :model-value="textFor(path)"
        size="small"
        :placeholder="scalarDefault(spec) ?? undefined"
        :data-testid="`row-editor-scalar-${path}`"
        @update:model-value="(value: string | undefined) => setText(path, value ?? '')"
      />
    </label>

    <Message
      v-if="saveError"
      severity="error"
      data-testid="row-editor-error"
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

    <div class="flex gap-2">
      <Button
        :label="t('assignments.rowEditor.saveRowButton')"
        size="small"
        :loading="isSaving"
        data-testid="row-editor-save-button"
        @click="save"
      />
      <Button
        v-if="existingRow"
        :label="t('assignments.rowEditor.clearRowButton')"
        size="small"
        severity="secondary"
        :loading="isClearing"
        data-testid="row-editor-clear-button"
        @click="clear"
      />
    </div>
  </div>
</template>
