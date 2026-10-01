<script setup lang="ts">
/**
 * The "Create pair rule" control for one predicted-failure row, rendered
 * once per **distinct** reorder that row's def targets ask for.
 *
 * Its own component because both verify sections need it — the fully
 * order-fixable list *and* the grouped-by-mod list, whose operations can
 * still carry a reorder for some of their def targets. Gating the
 * button on the display-level "every def target is order-fixable"
 * judgment hid it from exactly those rows while the CLI printed a
 * `fix:` line for them.
 *
 * Purely presentational: it never calls a mutation itself, so the dialog
 * keeps one place where a rule is written and one place where the
 * created/existing state lives.
 */
import Button from "primevue/button";
import { useI18n } from "vue-i18n";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { VerifyReorderDto } from "@/types/generated/VerifyReorderDto";
import { edgeKindLabel } from "@/utils/edgeKind";
import { edgeStatusLabel } from "@/utils/edgeStatus";
import type { CommandErrorDescriptor } from "@/utils/errors";
import { rationaleText } from "@/utils/rationale";

const { rowKey, reorders, createdBy, existingRules, errors, pending } = defineProps<{
  /** Identifies the row these buttons belong to — see {@link createdBy}. */
  rowKey: string;
  /** The distinct reorders this row asks for, already deduplicated. */
  reorders: readonly VerifyReorderDto[];
  /** `pairKey` -> the `rowKey` that created that rule in this pass. */
  createdBy: ReadonlyMap<string, string>;
  /**
   * `pairKey`s that already had a `userDecision` rule when this verify
   * report was produced — a **snapshot**, never a live read, so creating
   * a rule here cannot retroactively relabel the row that created it.
   */
  existingRules: ReadonlySet<string>;
  /** `pairKey` -> a failed `upsert_rule`. */
  errors: ReadonlyMap<string, CommandErrorDescriptor>;
  /** True while any `upsert_rule` is in flight. */
  pending: boolean;
}>();

const emit = defineEmits<{ create: [reorder: VerifyReorderDto] }>();

const { t, locale } = useI18n();
const modLabel = useModLabel();
const tm = useTranslateMessage();

/** `after|before` — one pair rule's identity, independent of the row offering it. */
function pairKeyOf(reorder: VerifyReorderDto): string {
  return `${reorder.after}|${reorder.before}`;
}

/**
 * Which of the two confirmations to show, or `null` for none yet.
 *
 * A pair that already had a rule when this report was produced always
 * reads "already exists" — before *and* after a click, since clicking
 * only re-affirms it (the dialog preserves its `overridesDeclared` and
 * its comment). "Created" is reserved for a rule this row genuinely
 * brought into being; another row asking for that same relation reads
 * "already exists" too, because by then it does.
 */
function stateOf(reorder: VerifyReorderDto): "created" | "exists" | null {
  const pairKey = pairKeyOf(reorder);
  if (existingRules.has(pairKey)) {
    return "exists";
  }
  const owner = createdBy.get(pairKey);
  if (owner === undefined) {
    return null;
  }
  return owner === rowKey ? "created" : "exists";
}

/**
 * Disabled once *this pass* has written the rule (a second write would
 * only rewrite the same row) or while any write is in flight. A rule
 * that merely pre-existed leaves the button live: clicking it is a
 * no-op-shaped re-affirmation that preserves what is already stored,
 * and disabling it would hide the one control that says so.
 */
function isDisabled(reorder: VerifyReorderDto): boolean {
  return pending || createdBy.has(pairKeyOf(reorder));
}

/**
 * Whether `reorder` contradicts something already in the report — a
 * reversed declaration/satisfied fact, or a re-broken cycle the sorter
 * already resolved. Never hides the button when true: the user decides,
 * with the conflicts named so they can.
 */
function hasConflicts(reorder: VerifyReorderDto): boolean {
  return reorder.conflicts.length > 0;
}

/** "conflicts with <kind>: <detail> (<status>)" — one line per conflict. */
function conflictText(conflict: VerifyReorderDto["conflicts"][number]): string {
  return t("apply.reorderActions.conflictText", {
    kind: tm(edgeKindLabel(conflict.kind)),
    detail: conflict.detail,
    status: tm(edgeStatusLabel(conflict.status)),
  });
}

/** {@link errors}'s own entry for `reorder`, rendered — `""` when there is none (the `v-if` above already guards this). */
function errorText(reorder: VerifyReorderDto): string {
  const error = errors.get(pairKeyOf(reorder));
  if (!error) {
    return "";
  }
  return error.technicalDetail
    ? `${tm(error.detail)} — ${error.technicalDetail}`
    : tm(error.detail);
}
</script>

<template>
  <div
    v-for="reorder in reorders"
    :key="pairKeyOf(reorder)"
    class="flex flex-wrap items-center gap-2 pt-1 pl-3"
    :data-testid="`apply-dialog-verify-reorder-${reorder.after}-${reorder.before}`"
  >
    <Button
      :label="
        hasConflicts(reorder)
          ? t('apply.reorderActions.createPairRuleAnyway')
          : t('apply.reorderActions.createPairRule')
      "
      size="small"
      :severity="hasConflicts(reorder) ? 'warn' : 'secondary'"
      outlined
      class="shrink-0"
      :disabled="isDisabled(reorder)"
      :data-testid="`apply-dialog-verify-create-pair-rule-${reorder.after}-${reorder.before}`"
      @click="emit('create', reorder)"
    />
    <span class="text-text-muted">
      {{
        t("action.reorder", {
          after: modLabel.label(reorder.after),
          before: modLabel.label(reorder.before),
        })
      }}
    </span>
    <span
      class="text-text-muted basis-full pl-1 text-xs"
      :data-testid="`apply-dialog-verify-reorder-rationale-${reorder.after}-${reorder.before}`"
    >
      {{ rationaleText(reorder.rationaleCode, t, modLabel.label, locale) }}
    </span>
    <div
      v-if="hasConflicts(reorder)"
      class="flex basis-full flex-col gap-0.5 pl-1 text-status-danger"
      :data-testid="`apply-dialog-verify-reorder-conflicts-${reorder.after}-${reorder.before}`"
    >
      <span
        v-for="conflict in reorder.conflicts"
        :key="`${conflict.kind}:${conflict.detail}`"
      >{{
        conflictText(conflict)
      }}</span>
    </div>
    <span
      v-if="stateOf(reorder) === 'created'"
      class="text-text"
      :data-testid="`apply-dialog-verify-pair-rule-created-${reorder.after}-${reorder.before}`"
    >
      {{ t("apply.reorderActions.ruleCreated") }}
    </span>
    <span
      v-else-if="stateOf(reorder) === 'exists'"
      class="text-text-faint"
      :data-testid="`apply-dialog-verify-pair-rule-exists-${reorder.after}-${reorder.before}`"
    >
      {{ t("apply.reorderActions.ruleExists") }}
    </span>
    <span
      v-if="errors.get(pairKeyOf(reorder))"
      class="text-status-danger"
      :data-testid="`apply-dialog-verify-pair-rule-error-${reorder.after}-${reorder.before}`"
    >
      {{ errorText(reorder) }}
    </span>
  </div>
</template>
