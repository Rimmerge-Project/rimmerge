<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import Select from "primevue/select";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import ModPicker from "@/components/mods/ModPicker.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { PlacementDto } from "@/types/generated/PlacementDto";
import type { PlacementRuleDto } from "@/types/generated/PlacementRuleDto";
import type { RuleDto } from "@/types/generated/RuleDto";
import { placementLabel } from "@/utils/placement";
import { ruleOriginLabel } from "@/utils/rule";

/**
 * Creates a brand-new user-owned placement rule — the gap
 * `apps/cli`'s own `rule set-placement` closes headlessly, but
 * `RuleTable.vue` only ever let a user view, promote, or delete an
 * *existing* row. Mirrors `run_set_placement` exactly: always emits
 * `origin: "userDecision"`, and deliberately never blocks submitting for
 * a mod that already has a rule — `upsert_rule`'s own upsert-by-`modId`
 * semantics (`RulesPage.vue`'s job to call) already replace it, matching
 * "a second `set-placement` for the same mod id replaces rather than
 * duplicates" exactly. Adding a client-side "already pinned" guard here
 * would make the two surfaces disagree about what a conflict does; the
 * `existingPlacements` prop is read only to *disclose* a replace
 * up front, never to refuse one.
 */
const { visible, pending, existingPlacements } = defineProps<{
  visible: boolean;
  /**
   * True while the parent's own `upsert_rule` mutation is in flight —
   * disables the submit button so a double click (or a resubmit before
   * the first request settles) can never fire two upserts for the same
   * pick.
   */
  pending: boolean;
  /** The rule set's current placement rows, read only to disclose a replace (see above). */
  existingPlacements: PlacementRuleDto[];
}>();
const emit = defineEmits<{ "update:visible": [value: boolean]; add: [rule: RuleDto] }>();
const { t } = useI18n();
const tm = useTranslateMessage();

const PLACEMENT_VALUES: readonly PlacementDto[] = ["top", "bottom"];
// A plain (non-`readonly`) mutable array: PrimeVue's `Select` types its
// `options` prop as `any[]`, which a `const`-asserted tuple can't
// satisfy (`TagRuleEditor.vue`'s own `MODES` does the same). Resolved
// through `t()` reactively, so a locale switch updates the labels.
const placementOptions = computed<{ label: string; value: PlacementDto }[]>(() =>
  PLACEMENT_VALUES.map((value) => ({ label: tm(placementLabel(value)), value })),
);

const selectedMod = ref<ModRefDto | null>(null);
const placement = ref<PlacementDto | null>(null);
const comment = ref("");

const pickerMembers = computed(() => (selectedMod.value ? [selectedMod.value] : []));

/**
 * `ModPicker` is a multi-select chip list; this dialog only ever wants
 * one mod pinned, so picking a new one replaces the previous pick
 * instead of adding to it — the component itself is reused unmodified
 * (see its own doc comment on why this is a genuine, not a forced, fit).
 */
function handlePickerChange(members: ModRefDto[]): void {
  selectedMod.value = members.at(-1) ?? null;
}

/** The existing row for the selected mod, if any — disclosure only, never a block. */
const conflictingRule = computed<PlacementRuleDto | null>(() => {
  const modId = selectedMod.value?.modId;
  if (!modId) return null;
  return existingPlacements.find((rule) => rule.modId === modId) ?? null;
});

const canSubmit = computed(() => selectedMod.value !== null && placement.value !== null);

function reset(): void {
  selectedMod.value = null;
  placement.value = null;
  comment.value = "";
}

// Clears the form every time the dialog closes, regardless of why
// (Cancel, Esc/the X button, or the parent closing it after a
// successful add) — a single reset point instead of one per closing
// path, and "cancel leaves state untouched" for the *next* open falls
// out of it for free.
watch(
  () => visible,
  (isVisible) => {
    if (!isVisible) reset();
  },
);

function close(): void {
  emit("update:visible", false);
}

function submit(): void {
  if (!canSubmit.value || pending) return;
  const mod = selectedMod.value;
  const chosenPlacement = placement.value;
  if (!mod || !chosenPlacement) return;
  emit("add", {
    kind: "placement",
    modId: mod.modId,
    placement: chosenPlacement,
    origin: "userDecision",
    comment: comment.value.trim().length > 0 ? comment.value.trim() : null,
    promotedFrom: null,
    alreadyPromoted: false,
  });
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :header="t('rules.addPlacementDialog.header')"
    data-testid="add-rule-dialog"
    @update:visible="(value: boolean) => emit('update:visible', value)"
  >
    <form
      class="flex w-96 flex-col gap-3"
      data-testid="add-rule-form"
      @submit.prevent="submit"
    >
      <div class="flex flex-col gap-1 text-sm">
        {{ t("rules.addPlacementDialog.modToPinLabel") }}
        <ModPicker
          :members="pickerMembers"
          :target-label="t('rules.addPlacementDialog.modToPinLabel')"
          testid-prefix="add-rule-mod"
          data-testid="add-rule-mod-picker"
          @change="handlePickerChange"
        />
      </div>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.addPlacementDialog.placementLabel") }}
        <Select
          v-model="placement"
          :options="placementOptions"
          option-label="label"
          option-value="value"
          :placeholder="t('rules.addPlacementDialog.placementPlaceholder')"
          data-testid="add-rule-placement"
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.addPlacementDialog.commentLabel") }}
        <InputText
          v-model="comment"
          data-testid="add-rule-comment"
        />
      </label>
      <Message
        v-if="conflictingRule"
        severity="warn"
        data-testid="add-rule-replace-warning"
      >
        {{
          t("rules.addPlacementDialog.replaceWarning", {
            mod: selectedMod?.name,
            origin: tm(ruleOriginLabel(conflictingRule.origin)),
            placement: tm(placementLabel(conflictingRule.placement)),
          })
        }}
      </Message>
      <div class="flex justify-end gap-2">
        <Button
          type="button"
          :label="t('common.cancel')"
          severity="secondary"
          data-testid="add-rule-cancel"
          @click="close"
        />
        <Button
          type="submit"
          :label="t('rules.addPairDialog.addButton')"
          :disabled="!canSubmit"
          :loading="pending"
          data-testid="add-rule-submit"
        />
      </div>
    </form>
  </Dialog>
</template>
