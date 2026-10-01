<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import PairRuleTable from "@/components/rules/PairRuleTable.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { DeleteRuleRequestDto } from "@/types/generated/DeleteRuleRequestDto";
import type { PlacementRuleDto } from "@/types/generated/PlacementRuleDto";
import type { RuleKeyDto } from "@/types/generated/RuleKeyDto";
import type { RuleSetDto } from "@/types/generated/RuleSetDto";
import { placementLabel } from "@/utils/placement";
import { isImportedOrigin, ruleOriginLabel } from "@/utils/rule";

const {
  rules,
  useImportedPairs,
  useImportedPlacements,
  dependentCounts = {},
} = defineProps<{
  rules: RuleSetDto;
  /** Whether imported pair rules currently feed the sorter — greys imported pair rows when `false`. */
  useImportedPairs: boolean;
  /** Whether imported placement rules currently feed the sorter — greys imported placement rows when `false`. */
  useImportedPlacements: boolean;
  /**
   * Every pinned mod's own `PlacementPromotesDependents` count, keyed by
   * `modId` (`queries/rules.ts`'s own `usePlacementDependentCountsQuery`
   * — see its doc comment for why this can't be a pre-commit preview).
   * Absent from the map for a mod with no such finding, the common case
   * for an uncontested pin.
   */
  dependentCounts?: Record<string, number>;
}>();
const emit = defineEmits<{ delete: [request: DeleteRuleRequestDto]; promote: [key: RuleKeyDto] }>();
const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

const isEmpty = computed(
  () =>
    rules.pairs.length === 0 && rules.placements.length === 0 && rules.incompatibles.length === 0,
);

/** Greyed exactly when this is an imported row and its toggle is off. */
function isPlacementGreyedOut(placement: PlacementRuleDto): boolean {
  return isImportedOrigin(placement.origin) && !useImportedPlacements;
}

/**
 * A promoted copy shares its imported original's `modId` — both are
 * listed at once ("promote to user rule") — so `origin` must be part
 * of the row's identity, not just the mod, or two simultaneous rows would
 * collide on the same `:key`/`data-testid`.
 */
function placementRowId(placement: PlacementRuleDto): string {
  return `${placement.modId}-${placement.origin}`;
}

/** See the `dependentCounts` prop's own doc comment. `undefined` for the common, uncontested-pin case. */
function dependentCountFor(placement: PlacementRuleDto): number | undefined {
  return dependentCounts[placement.modId];
}

/** "N other mod(s)" for a placement with a dependent count — `""` (never rendered, guarded by the template's own `v-if`) when there is none. */
function otherModsText(placement: PlacementRuleDto): string {
  const count = dependentCountFor(placement);
  return count === undefined ? "" : t("rules.table.otherMods", { count }, count);
}
</script>

<template>
  <div
    class="flex flex-col gap-6"
    data-testid="rule-table"
  >
    <section v-if="rules.pairs.length > 0">
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("rules.table.pairRulesHeading") }}
      </h3>
      <PairRuleTable
        :pairs="rules.pairs"
        :use-imported-pairs="useImportedPairs"
        @delete="emit('delete', $event)"
        @promote="emit('promote', $event)"
      />
    </section>

    <section v-if="rules.placements.length > 0">
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("rules.table.placementRulesHeading") }}
      </h3>
      <div class="surface-card overflow-hidden">
        <table class="w-full border-collapse text-sm">
          <thead>
            <tr class="table-head text-left">
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.modHeader") }}
              </th>
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.placementHeader") }}
              </th>
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.originHeader") }}
              </th>
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.commentHeader") }}
              </th>
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.promotesHeader") }}
              </th>
              <th
                scope="col"
                class="py-1"
              />
              <th
                scope="col"
                class="py-1"
              />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="placement in rules.placements"
              :key="placementRowId(placement)"
              class="border-border-subtle border-b"
              :title="
                isPlacementGreyedOut(placement) ? t('rules.table.notAppliedPlacementsTitle') : undefined
              "
              :data-testid="`placement-rule-${placementRowId(placement)}`"
            >
              <td
                :class="{ 'opacity-50': isPlacementGreyedOut(placement) }"
                :title="modLabel.titleFor(placement.modId)"
              >
                <span
                  v-if="isPlacementGreyedOut(placement)"
                  class="sr-only"
                >{{ t("rules.table.notAppliedPlacementsTitle") }}</span>
                {{ modLabel.label(placement.modId) }}
              </td>
              <td :class="{ 'opacity-50': isPlacementGreyedOut(placement) }">
                {{ tm(placementLabel(placement.placement)) }}
              </td>
              <td :class="{ 'opacity-50': isPlacementGreyedOut(placement) }">
                {{ tm(ruleOriginLabel(placement.origin)) }}
                <span
                  v-if="placement.promotedFrom"
                  class="text-text-faint text-xs"
                  :data-testid="`placement-rule-${placementRowId(placement)}-promoted-from`"
                >
                  {{
                    t("rules.pairTable.promotedFrom", {
                      origin: tm(ruleOriginLabel(placement.promotedFrom)),
                    })
                  }}
                </span>
              </td>
              <td :class="{ 'opacity-50': isPlacementGreyedOut(placement) }">
                {{ placement.comment }}
              </td>
              <td :class="{ 'opacity-50': isPlacementGreyedOut(placement) }">
                <span
                  v-if="dependentCountFor(placement) !== undefined"
                  class="text-status-input text-xs"
                  :data-testid="`placement-rule-${placementRowId(placement)}-promotes`"
                >
                  {{ otherModsText(placement) }}
                </span>
              </td>
              <td>
                <Button
                  v-if="isImportedOrigin(placement.origin)"
                  :label="
                    placement.alreadyPromoted
                      ? t('rules.pairTable.promotedButton')
                      : t('rules.pairTable.promoteButton')
                  "
                  size="small"
                  text
                  :disabled="placement.alreadyPromoted"
                  :data-testid="`placement-rule-${placementRowId(placement)}-promote`"
                  @click="emit('promote', { kind: 'placement', modId: placement.modId })"
                />
              </td>
              <td>
                <Button
                  :label="t('rules.pairTable.deleteButton')"
                  size="small"
                  severity="danger"
                  text
                  @click="
                    emit('delete', {
                      key: { kind: 'placement', modId: placement.modId },
                      origin: placement.origin,
                    })
                  "
                />
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <section v-if="rules.incompatibles.length > 0">
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("rules.table.incompatibilitiesHeading") }}
      </h3>
      <div class="surface-card overflow-hidden">
        <table class="w-full border-collapse text-sm">
          <thead>
            <tr class="table-head text-left">
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.aHeader") }}
              </th>
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.bHeader") }}
              </th>
              <th
                scope="col"
                class="py-1 font-medium"
              >
                {{ t("rules.table.originHeader") }}
              </th>
              <th
                scope="col"
                class="py-1"
              />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="pair in rules.incompatibles"
              :key="`${pair.a}|${pair.b}|${pair.origin}`"
              class="border-border-subtle border-b"
              :data-testid="`incompatible-rule-${pair.a}-${pair.b}`"
            >
              <td :title="modLabel.titleFor(pair.a)">
                {{ modLabel.label(pair.a) }}
              </td>
              <td :title="modLabel.titleFor(pair.b)">
                {{ modLabel.label(pair.b) }}
              </td>
              <td>{{ tm(ruleOriginLabel(pair.origin)) }}</td>
              <td>
                <Button
                  :label="t('rules.pairTable.deleteButton')"
                  size="small"
                  severity="danger"
                  text
                  @click="
                    emit('delete', {
                      key: { kind: 'incompatible', a: pair.a, b: pair.b },
                      origin: pair.origin,
                    })
                  "
                />
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <p
      v-if="isEmpty"
      class="text-text-muted text-sm"
      data-testid="rule-table-empty"
    >
      {{ t("rules.table.empty") }}
    </p>
  </div>
</template>
