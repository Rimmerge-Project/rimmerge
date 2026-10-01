<script setup lang="ts">
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import ModPicker from "@/components/mods/ModPicker.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { PairRuleDto } from "@/types/generated/PairRuleDto";
import type { RuleDto } from "@/types/generated/RuleDto";
import { ruleOriginLabel } from "@/utils/rule";

/**
 * Creates a brand-new user-owned pair rule — `set-pair`'s desktop
 * sibling (a user needs a supported way to state "A before B" for a pair
 * no rule database mentions, such as a mutual `patch_removed_node`
 * 2-cycle the sorter would otherwise break alphabetically). Mirrors
 * `AddPlacementRuleDialog.vue` exactly:
 * always emits `origin: "userDecision"`, and deliberately never blocks
 * submitting for a pair that already has a rule — `upsert_rule`'s own
 * upsert-by-key semantics (`RulesPage.vue`'s job to call) already replace
 * it, matching `apps/cli`'s `rule set-pair` "a second call for the same
 * pair replaces the first" exactly. The `existingPairs` prop is read only
 * to *disclose* a replace up front, never to refuse one — same reasoning
 * as the placement dialog's own `existingPlacements`.
 */
const { visible, pending, existingPairs } = defineProps<{
  visible: boolean;
  /**
   * True while the parent's own `upsert_rule` mutation is in flight —
   * disables the submit button so a double click (or a resubmit before
   * the first request settles) can never fire two upserts for the same
   * pick.
   */
  pending: boolean;
  /** The rule set's current pair rows, read only to disclose a replace (see above). */
  existingPairs: PairRuleDto[];
}>();
const emit = defineEmits<{ "update:visible": [value: boolean]; add: [rule: RuleDto] }>();
const { t } = useI18n();
const tm = useTranslateMessage();

const afterMod = ref<ModRefDto | null>(null);
const beforeMod = ref<ModRefDto | null>(null);
const comment = ref("");
/**
 * The declared-edge override (deferred half of
 * the pair-rule feature): loud, explicit, per-pair opt-in — never a
 * default — for this rule to outrank a `Declared`-strength author edge
 * (`loadAfter`/`modDependencies`) between the same two mods, should one
 * exist. Still loses to any hard dependency regardless.
 */
const overrideDeclared = ref(false);

const afterPickerMembers = computed(() => (afterMod.value ? [afterMod.value] : []));
const beforePickerMembers = computed(() => (beforeMod.value ? [beforeMod.value] : []));

/** See `AddPlacementRuleDialog.vue`'s identical helper — one mod at a time, replacing the previous pick. */
function handleAfterChange(members: ModRefDto[]): void {
  afterMod.value = members.at(-1) ?? null;
}
function handleBeforeChange(members: ModRefDto[]): void {
  beforeMod.value = members.at(-1) ?? null;
}

/** The two picks name the same mod — a pair rule needs two distinct ones. */
const sameModOnBothSides = computed(
  () =>
    afterMod.value !== null &&
    beforeMod.value !== null &&
    afterMod.value.modId === beforeMod.value.modId,
);

/** The existing row for the selected pair, if any — disclosure only, never a block. */
const conflictingRule = computed<PairRuleDto | null>(() => {
  const after = afterMod.value?.modId;
  const before = beforeMod.value?.modId;
  if (!after || !before) return null;
  return existingPairs.find((rule) => rule.after === after && rule.before === before) ?? null;
});

const canSubmit = computed(
  () => afterMod.value !== null && beforeMod.value !== null && !sameModOnBothSides.value,
);

function reset(): void {
  afterMod.value = null;
  beforeMod.value = null;
  comment.value = "";
  overrideDeclared.value = false;
}

// Same single reset point as `AddPlacementRuleDialog.vue` — every closing
// path (Cancel, Esc, a successful add) clears the form, and "cancel
// leaves state untouched" for the next open falls out of it for free.
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
  const after = afterMod.value;
  const before = beforeMod.value;
  if (!after || !before) return;
  emit("add", {
    kind: "pair",
    after: after.modId,
    before: before.modId,
    origin: "userDecision",
    comment: comment.value.trim().length > 0 ? comment.value.trim() : null,
    promotedFrom: null,
    alreadyPromoted: false,
    overridesDeclared: overrideDeclared.value,
  });
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :header="t('rules.addPairDialog.header')"
    data-testid="add-pair-rule-dialog"
    @update:visible="(value: boolean) => emit('update:visible', value)"
  >
    <form
      class="flex w-96 flex-col gap-3"
      data-testid="add-pair-rule-form"
      @submit.prevent="submit"
    >
      <div class="flex flex-col gap-1 text-sm">
        {{ t("rules.addPairDialog.loadsAfterLabel") }}
        <ModPicker
          :members="afterPickerMembers"
          :target-label="t('rules.addPairDialog.loadsAfterLabel')"
          testid-prefix="add-pair-rule-after"
          data-testid="add-pair-rule-after-picker"
          @change="handleAfterChange"
        />
      </div>
      <div class="flex flex-col gap-1 text-sm">
        {{ t("rules.addPairDialog.loadsBeforeLabel") }}
        <ModPicker
          :members="beforePickerMembers"
          :target-label="t('rules.addPairDialog.loadsBeforeLabel')"
          testid-prefix="add-pair-rule-before"
          data-testid="add-pair-rule-before-picker"
          @change="handleBeforeChange"
        />
      </div>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.addPairDialog.commentLabel") }}
        <InputText
          v-model="comment"
          data-testid="add-pair-rule-comment"
        />
      </label>
      <label class="flex items-center gap-2 text-sm">
        <Checkbox
          v-model="overrideDeclared"
          class="shrink-0"
          binary
          data-testid="add-pair-rule-override-declared"
        />
        {{ t("rules.addPairDialog.overrideDeclaredLabel") }}
      </label>
      <Message
        v-if="overrideDeclared"
        severity="warn"
        data-testid="add-pair-rule-override-declared-warning"
      >
        {{ t("rules.addPairDialog.overrideDeclaredWarning") }}
      </Message>
      <Message
        v-if="sameModOnBothSides"
        severity="error"
        data-testid="add-pair-rule-same-mod-error"
      >
        {{ t("rules.addPairDialog.sameModError") }}
      </Message>
      <Message
        v-if="conflictingRule"
        severity="warn"
        data-testid="add-pair-rule-replace-warning"
      >
        {{
          t("rules.addPairDialog.replaceWarning", { origin: tm(ruleOriginLabel(conflictingRule.origin)) })
        }}
      </Message>
      <div class="flex justify-end gap-2">
        <Button
          type="button"
          :label="t('common.cancel')"
          severity="secondary"
          data-testid="add-pair-rule-cancel"
          @click="close"
        />
        <Button
          type="submit"
          :label="t('rules.addPairDialog.addButton')"
          :disabled="!canSubmit"
          :loading="pending"
          data-testid="add-pair-rule-submit"
        />
      </div>
    </form>
  </Dialog>
</template>
