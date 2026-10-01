<script setup lang="ts">
import Button from "primevue/button";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import BaseConfidence from "@/components/base/BaseConfidence.vue";
import BaseStatusPill from "@/components/base/BaseStatusPill.vue";
import AlternativeList from "@/components/inbox/AlternativeList.vue";
import DecisionNote from "@/components/inbox/DecisionNote.vue";
import DefConflictView from "@/components/inbox/DefConflictView.vue";
import TexturePair from "@/components/inbox/TexturePair.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { DefRefPartsDto } from "@/types/generated/DefRefPartsDto";
import type { PatchFailureCauseDto } from "@/types/generated/PatchFailureCauseDto";
import type { RefSiteReferrerDto } from "@/types/generated/RefSiteReferrerDto";
import type { RefSiteSummaryDto } from "@/types/generated/RefSiteSummaryDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import { assertNever } from "@/utils/assertNever";
import { danglingCauseText } from "@/utils/danglingCause";
import { edgeKindLabel } from "@/utils/edgeKind";
import { edgeStrengthLabel } from "@/utils/edgeStrength";
import { describeAction, inspectRoute } from "@/utils/format";
import { layerLabel } from "@/utils/layer";
import { placementLabel } from "@/utils/placement";
import { rationaleText } from "@/utils/rationale";
import { ruleOriginLabel } from "@/utils/rule";
import { tagSignalLabel } from "@/utils/tagSignal";

const {
  detail,
  noteOpen = false,
  patchId = null,
} = defineProps<{
  detail: ResolutionDetailDto;
  /** Whether the note editor is expanded (driven by the inbox store's `expandedKey`). */
  noteOpen?: boolean;
  /** Forwarded to `DefConflictView` — see its own `patchId` doc comment. `null` for the profile inbox. */
  patchId?: string | null;
}>();

const emit = defineEmits<{
  accept: [];
  pickAlternative: [oneBasedIndex: number];
  ignore: [];
  revert: [];
  openNote: [];
  saveNote: [note: string];
  closeNote: [];
}>();

const finding = computed(() => detail.finding);
const { t, locale } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

/**
 * `PlacementPromotesDependents.promoted` can be large on a real
 * install — 114 for a framework-shaped pin (`example.framework`) — so it
 * renders capped, behind the same "Show more (X of Y)" affordance
 * `usePagedRows`'s own consumers (`DefConflictView.vue`/`ModChanges.vue`/
 * `DefPage.vue`) already use for a long list; not `usePagedRows` itself,
 * since `promoted` arrives whole in one `FindingDto` (there is nothing
 * left to fetch a further page of, only more of what's already in hand
 * to reveal).
 */
const PROMOTED_PREVIEW_LIMIT = 20;
const promotedExpanded = ref(false);
// Reset whenever the finding this panel is showing changes — the panel
// is reused across findings (`usePagedRows`'s reset handling exists for
// exactly this reuse pattern), so a
// prior expand choice must never carry over to unrelated content.
watch(
  () => detail.key,
  () => {
    promotedExpanded.value = false;
  },
);
const promotedList = computed<string[]>(() =>
  finding.value.kind === "placementPromotesDependents" ? finding.value.promoted : [],
);
const visiblePromoted = computed(() =>
  promotedExpanded.value ? promotedList.value : promotedList.value.slice(0, PROMOTED_PREVIEW_LIMIT),
);

/**
 * `BrokenInheritance.affected` preview cap — a plain static "+N more"
 * line, not an expandable list like {@link visiblePromoted}: the server
 * already bounds `affected` at 500 (`MAX_AFFECTED_DEFS`,
 * `analysis::inheritance`), so a client-side expand button would only
 * ever reveal a handful more entries in practice, not worth the extra
 * reactive state.
 */
const BROKEN_INHERITANCE_AFFECTED_PREVIEW_LIMIT = 20;

/**
 * The def-inspector ref text for a `BrokenInheritance` finding's own
 * `child` — a `defName` address for a concrete def, or the `@name`
 * template-address form for an abstract one (mirrors
 * `rim_resolve::domain::DefKey`'s `Display`/`DefRef`'s own `@`
 * convention).
 */
function brokenInheritanceChildDefRef(child: DefRefPartsDto): string {
  return child.isTemplate ? `${child.defType}/@${child.name}` : `${child.defType}/${child.name}`;
}

/** Which `inbox.evidence.patchWillFail.*` whole-sentence key a `PatchFailureCauseDto` selects. Exhaustive via {@link assertNever}. */
function patchWillFailKeypath(cause: PatchFailureCauseDto): string {
  switch (cause.kind) {
    case "removedBy":
      return "inbox.evidence.patchWillFail.causeRemovedBy";
    case "notYetInjected":
      return "inbox.evidence.patchWillFail.causeNotYetInjected";
    case "deadTarget":
      return "inbox.evidence.patchWillFail.causeDeadTarget";
    case "unknown":
      return "inbox.evidence.patchWillFail.causeUnclassified";
    default:
      return assertNever(cause);
  }
}

/** Human text for one `DanglingDefReference` finding's own referrer. */
function danglingReferrerText(summary: RefSiteSummaryDto, label: (id: string) => string): string {
  const referrer: RefSiteReferrerDto = summary.referrer;
  switch (referrer.kind) {
    case "referencedFromDef":
      return t("inbox.evidence.danglingDefReference.referrerFromDef", {
        modId: label(referrer.modId),
        defType: referrer.defType,
        defName: referrer.defName,
        fieldPath: summary.fieldPath,
      });
    case "referencedFromTemplate":
      return t("inbox.evidence.danglingDefReference.referrerFromTemplate", {
        modId: label(referrer.modId),
        defType: referrer.defType,
        name: referrer.name,
        fieldPath: summary.fieldPath,
      });
    case "referencedFromPatch":
      return t("inbox.evidence.danglingDefReference.referrerFromPatch", {
        modId: label(referrer.modId),
        defType: referrer.defType,
        defName: referrer.defName,
        fieldPath: summary.fieldPath,
      });
    default:
      return assertNever(referrer);
  }
}

/** One owner row in the runtime-patch block: whose patch runs last today, and how to force another one to. */
interface RuntimePatchRow {
  owner: string;
  isLastPatcher: boolean;
  /**
   * The 1-based `pickAlternative` index for this owner's `PreferWinner`
   * alternative — `-1` (never rendered as a button; guarded by the
   * template's own `> 0` check) when `ledger::suggest::
   * runtime_patch_collision` didn't offer one, which shouldn't happen
   * since it generates exactly one per owner.
   */
  alternativeIndex: number;
}

/**
 * `finding.owners` is documented as "every patching mod, in load order"
 * (`FindingDto::RuntimePatchCollision`) — the same order
 * `runtime_patch_collision`'s own "last patcher" rationale is computed
 * from — so the last element is the mod whose patch runs last today.
 */
const runtimePatchRows = computed<RuntimePatchRow[]>(() => {
  if (finding.value.kind !== "runtimePatchCollision") {
    return [];
  }
  const owners = finding.value.owners;
  const alternatives = detail.suggestion.alternatives;
  return owners.map((owner, index) => {
    const altIndex = alternatives.findIndex(
      (alternative) =>
        alternative.action.kind === "preferWinner" && alternative.action.winner === owner,
    );
    return {
      owner,
      isLastPatcher: index === owners.length - 1,
      alternativeIndex: altIndex === -1 ? -1 : altIndex + 1,
    };
  });
});
</script>

<template>
  <div
    class="flex flex-col gap-4 p-4"
    data-testid="suggestion-panel"
  >
    <header class="flex items-center justify-between gap-2">
      <h2
        class="text-text-faint truncate font-mono text-[11px]"
        :title="detail.key"
      >
        {{ detail.key }}
      </h2>
      <div class="flex shrink-0 items-center gap-2">
        <RouterLink
          v-if="detail.defRef"
          :to="inspectRoute(detail.defRef)"
          class="text-accent text-xs underline"
          data-testid="suggestion-panel-inspect-link"
        >
          {{ t("inbox.evidence.inspectLink") }}
        </RouterLink>
        <BaseStatusPill :status="detail.status" />
      </div>
    </header>

    <section
      data-testid="finding-payload"
      :aria-label="t('inbox.evidence.ariaLabel')"
    >
      <template v-if="finding.kind === 'edgeDropped'">
        <i18n-t
          keypath="inbox.evidence.edgeDropped.sentence"
          tag="p"
        >
          <template #strength>
            <strong>{{ tm(edgeStrengthLabel(finding.strength)) }}</strong>
          </template>
          <template #edgeKind>
            {{ tm(edgeKindLabel(finding.edgeKind)) }}
          </template>
          <template #after>
            {{ modLabel.label(finding.after) }}
          </template>
          <template #before>
            {{ modLabel.label(finding.before) }}
          </template>
        </i18n-t>
        <p class="text-text-muted text-sm">
          {{ finding.detail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'anyOfChoice'">
        <p>
          {{
            t("inbox.evidence.anyOfChoice.intro", {
              modId: modLabel.label(finding.after),
              assembly: finding.assembly,
            })
          }}
        </p>
        <ul class="list-inside list-disc text-sm">
          <li
            v-for="candidate in finding.candidates"
            :key="candidate"
          >
            {{ modLabel.label(candidate) }}
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'defOverride' || finding.kind === 'patchCollision'">
        <DefConflictView
          :finding="finding"
          :finding-key="detail.key"
          :patch-id="patchId"
        />
      </template>

      <template v-else-if="finding.kind === 'textureOverride'">
        <i18n-t
          keypath="inbox.evidence.textureOverride.sentence"
          tag="p"
          class="font-medium"
        >
          <template #path>
            <code>{{ finding.texturePath }}</code>
          </template>
        </i18n-t>
        <TexturePair
          :finding="finding"
          :alternatives="detail.suggestion.alternatives"
          @pick-alternative="(index) => emit('pickAlternative', index)"
        />
      </template>

      <template v-else-if="finding.kind === 'duplicateAssembly'">
        <i18n-t
          keypath="inbox.evidence.duplicateAssembly.sentence"
          tag="p"
        >
          <template #name>
            <code>{{ finding.assemblyName }}</code>
          </template>
        </i18n-t>
        <ul class="list-inside list-disc text-sm">
          <li
            v-for="owner in finding.owners"
            :key="owner"
            :title="modLabel.titleFor(owner)"
          >
            {{ modLabel.label(owner) }}
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'likelyDuplicateMod'">
        <p>
          {{
            t("inbox.evidence.likelyDuplicateMod.body", {
              a: modLabel.label(finding.a),
              b: modLabel.label(finding.b),
              count: finding.sharedDefs,
            }, finding.sharedDefs)
          }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'missingMod'">
        <p>{{ t("inbox.evidence.missingMod.body", { modId: modLabel.label(finding.modId) }) }}</p>
      </template>

      <template v-else-if="finding.kind === 'missingDependency'">
        <p>
          {{
            t("inbox.evidence.missingDependency.body", {
              modId: modLabel.label(finding.modId),
              dependencyLabel: modLabel.labelWithFallback(finding.dependency, finding.displayName),
              dependency: finding.dependency,
            })
          }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'incompatiblePair'">
        <p>
          {{
            t("inbox.evidence.incompatiblePair.body", {
              a: modLabel.label(finding.a),
              b: modLabel.label(finding.b),
            })
          }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'unsupportedVersion'">
        <p>
          {{ t("inbox.evidence.unsupportedVersion.body", { modId: modLabel.label(finding.modId) }) }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'undeclaredHardDependency'">
        <p>
          {{
            t("inbox.evidence.undeclaredHardDependency.body", {
              after: modLabel.label(finding.after),
              before: modLabel.label(finding.before),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ finding.detail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'lazyReferenceViolated'">
        <p>
          {{
            t("inbox.evidence.lazyReferenceViolated.body", {
              after: modLabel.label(finding.after),
              before: modLabel.label(finding.before),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ finding.detail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'declarationQuestioned'">
        <p>
          {{
            t("inbox.evidence.declarationQuestioned.body", {
              declaredAfter: modLabel.label(finding.declaredAfter),
              declaredBefore: modLabel.label(finding.declaredBefore),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ finding.relationDetail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'declarationOverridden'">
        <p>
          {{
            t("inbox.evidence.declarationOverridden.body", {
              declaredAfter: modLabel.label(finding.declaredAfter),
              declaredBefore: modLabel.label(finding.declaredBefore),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ modLabel.label(finding.by.after) }} → {{ modLabel.label(finding.by.before) }}
          ({{ tm(layerLabel(finding.by.layer)) }}): {{ finding.by.detail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'duplicateTemplateName'">
        <DefConflictView
          :finding="finding"
          :finding-key="detail.key"
          :patch-id="patchId"
        />
      </template>

      <template v-else-if="finding.kind === 'keyedTranslationCollision'">
        <p>
          {{
            t(
              "inbox.evidence.keyedTranslationCollision.body",
              { a: modLabel.label(finding.a), b: modLabel.label(finding.b), count: finding.keys.length },
              finding.keys.length,
            )
          }}
        </p>
        <ul class="list-inside list-disc text-sm">
          <li
            v-for="key in finding.keys"
            :key="key"
          >
            {{ key }}
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'soundOverride'">
        <i18n-t
          keypath="inbox.evidence.soundOverride.sentence"
          tag="p"
        >
          <template #path>
            <code>{{ finding.path }}</code>
          </template>
        </i18n-t>
        <ul class="list-inside list-disc text-sm">
          <li
            v-for="owner in finding.owners"
            :key="owner"
            :title="modLabel.titleFor(owner)"
          >
            {{ modLabel.label(owner) }}
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'undeclaredTypeDependency'">
        <i18n-t
          keypath="inbox.evidence.undeclaredTypeDependency.sentence"
          tag="p"
        >
          <template #user>
            {{ modLabel.label(finding.user) }}
          </template>
          <template #typeName>
            <code>{{ finding.typeName }}</code>
          </template>
          <template #provider>
            {{ modLabel.label(finding.provider) }}
          </template>
        </i18n-t>
      </template>

      <template v-else-if="finding.kind === 'runtimePatchCollision'">
        <i18n-t
          keypath="inbox.evidence.runtimePatchCollision.sentence"
          tag="p"
        >
          <template #target>
            <code>{{ finding.targetType }}::{{ finding.targetMethod }}</code>
          </template>
        </i18n-t>
        <ul class="flex flex-col gap-1 text-sm">
          <li
            v-for="row in runtimePatchRows"
            :key="row.owner"
            class="flex items-center gap-2"
          >
            <span :title="modLabel.titleFor(row.owner)">{{ modLabel.label(row.owner) }}</span>
            <span
              v-if="row.isLastPatcher"
              class="bg-surface-2 text-text-muted rounded px-1.5 py-0.5 text-xs"
              data-testid="runtime-patch-last-patcher"
            >
              {{ t("inbox.evidence.runtimePatchCollision.runsLast") }}
            </span>
            <Button
              v-else-if="row.alternativeIndex > 0"
              :label="t('inbox.evidence.runtimePatchCollision.runLastButton')"
              size="small"
              text
              :aria-label="
                t('inbox.evidence.runtimePatchCollision.runLastAriaLabel', {
                  modId: modLabel.label(row.owner),
                })
              "
              :data-testid="`runtime-patch-run-last-${row.owner}`"
              @click="emit('pickAlternative', row.alternativeIndex)"
            />
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'transpilerCollision'">
        <i18n-t
          keypath="inbox.evidence.transpilerCollision.sentence"
          tag="p"
        >
          <template #target>
            <code>{{ finding.targetType }}::{{ finding.targetMethod }}</code>
          </template>
        </i18n-t>
        <ul class="list-inside list-disc text-sm">
          <li
            v-for="owner in finding.owners"
            :key="owner"
            :title="modLabel.titleFor(owner)"
          >
            {{ modLabel.label(owner) }}
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'tagInferred'">
        <i18n-t
          keypath="inbox.evidence.tagInferred.sentence"
          tag="p"
        >
          <template #modId>
            {{ modLabel.label(finding.modId) }}
          </template>
          <template #tag>
            <code>{{ finding.tag }}</code>
          </template>
        </i18n-t>
        <ul class="list-inside list-disc text-sm">
          <li
            v-for="signal in finding.matched"
            :key="`${signal.kind}:${signal.kind === 'urlContains' ? signal.needle : signal.modId}`"
          >
            {{ tagSignalLabel(signal, t, modLabel.label) }}
          </li>
        </ul>
      </template>

      <template v-else-if="finding.kind === 'ruleOverruled'">
        <p>
          {{
            t(
              finding.winner
                ? "inbox.evidence.ruleOverruled.withWinner"
                : "inbox.evidence.ruleOverruled.byLongerCycle",
              {
                after: modLabel.label(finding.after),
                before: modLabel.label(finding.before),
                origin: tm(ruleOriginLabel(finding.origin)),
              },
            )
          }}
        </p>
        <p
          v-if="finding.winner"
          class="text-text-muted text-sm"
        >
          {{ modLabel.label(finding.winner.after) }} → {{ modLabel.label(finding.winner.before) }}
          ({{ tm(layerLabel(finding.winner.layer)) }}): {{ finding.winner.detail }}
        </p>
        <p
          v-else
          class="text-text-faint text-xs"
          data-testid="rule-overruled-witness-cycle"
        >
          {{
            t("inbox.evidence.ruleOverruled.cycle", {
              chain: finding.witnessCycle.map((id) => modLabel.label(id)).join(" → "),
            })
          }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'placementOverruled'">
        <p>
          {{
            t("inbox.evidence.placementOverruled.intro", {
              modId: modLabel.label(finding.modId),
              placement: tm(placementLabel(finding.placement)),
              origin: tm(ruleOriginLabel(finding.origin)),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ modLabel.label(finding.by.after) }} → {{ modLabel.label(finding.by.before) }}
          ({{ tm(layerLabel(finding.by.layer)) }}): {{ finding.by.detail }}
        </p>
        <p class="text-text-muted text-sm">
          {{ t("inbox.evidence.placementOverruled.landedAt", { position: finding.landedAt + 1 }) }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'placementQuestioned'">
        <p>
          {{
            t("inbox.evidence.placementQuestioned.body", {
              modId: modLabel.label(finding.modId),
              placement: tm(placementLabel(finding.placement)),
              relationKind: tm(edgeKindLabel(finding.relationKind)),
              other: modLabel.label(finding.other),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ finding.relationDetail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'placementOrderingOverridden'">
        <p>
          {{
            t("inbox.evidence.placementOrderingOverridden.body", {
              modId: modLabel.label(finding.modId),
              pinned: modLabel.label(finding.pinned),
              placement: tm(placementLabel(finding.placement)),
            })
          }}
        </p>
        <p class="text-text-muted text-sm">
          {{ modLabel.label(finding.by.after) }} → {{ modLabel.label(finding.by.before) }}
          ({{ tm(layerLabel(finding.by.layer)) }}): {{ finding.by.detail }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'placementPromotesDependents'">
        <p>
          {{
            t(
              "inbox.evidence.placementPromotesDependents.body",
              {
                modId: modLabel.label(finding.modId),
                placement: tm(placementLabel(finding.placement)),
                count: finding.promoted.length,
              },
              finding.promoted.length,
            )
          }}
        </p>
        <ul
          class="list-inside list-disc text-sm"
          data-testid="placement-promotes-dependents-list"
        >
          <li
            v-for="promotedId in visiblePromoted"
            :key="promotedId"
            :title="modLabel.titleFor(promotedId)"
          >
            {{ modLabel.label(promotedId) }}
          </li>
        </ul>
        <button
          v-if="visiblePromoted.length < promotedList.length"
          type="button"
          class="border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text cursor-pointer self-start rounded border px-2 py-1 text-xs"
          data-testid="placement-promotes-dependents-show-more"
          @click="promotedExpanded = true"
        >
          {{
            t("inbox.evidence.showMore", {
              shown: visiblePromoted.length,
              total: promotedList.length,
            })
          }}
        </button>
      </template>

      <template v-else-if="finding.kind === 'missingTexturePath'">
        <i18n-t
          keypath="inbox.evidence.missingTexturePath.sentence"
          tag="p"
        >
          <template #modId>
            {{ modLabel.label(finding.referrer) }}
          </template>
          <template #def>
            <code>{{ finding.def.defType }}/{{ finding.def.defName }}</code>
          </template>
          <template #field>
            <code>{{ finding.field }}</code>
          </template>
          <template #path>
            <code>{{ finding.path }}</code>
          </template>
        </i18n-t>
      </template>
      <template v-else-if="finding.kind === 'undecodableTexture'">
        <p>
          <i18n-t
            keypath="inbox.evidence.undecodableTexture.sentence"
            tag="span"
          >
            <template #modId>
              {{ modLabel.label(finding.modId) }}
            </template>
            <template #path>
              <code>{{ finding.path }}</code>
            </template>
            <template #width>
              {{ finding.width }}
            </template>
            <template #height>
              {{ finding.height }}
            </template>
            <template #fourcc>
              {{ finding.fourcc }}
            </template>
          </i18n-t>
          <template v-if="finding.hasPngSibling">
            {{ t("inbox.evidence.undecodableTexture.pngSibling") }}
          </template>
        </p>
      </template>
      <template v-else-if="finding.kind === 'brokenInheritance'">
        <i18n-t
          :keypath="
            finding.problem.kind === 'parentTypeMismatch'
              ? 'inbox.evidence.brokenInheritance.wrongType'
              : 'inbox.evidence.brokenInheritance.missingParent'
          "
          tag="p"
        >
          <template #modId>
            {{ modLabel.label(finding.modId) }}
          </template>
          <template #child>
            <RouterLink
              class="underline"
              :to="inspectRoute(brokenInheritanceChildDefRef(finding.child))"
            >
              <code>{{ finding.child.defType }}/{{
                finding.child.isTemplate ? `@${finding.child.name}` : finding.child.name
              }}</code>
            </RouterLink>
          </template>
          <template #parentName>
            <code>{{ finding.parentName }}</code>
          </template>
          <template
            v-if="finding.problem.kind === 'parentTypeMismatch'"
            #parentType
          >
            <code>{{ finding.problem.parentType }}</code>
          </template>
          <template
            v-if="finding.problem.kind === 'parentTypeMismatch'"
            #childDefType
          >
            <code>{{ finding.child.defType }}</code>
          </template>
        </i18n-t>
        <p
          v-if="finding.affected.length > 0"
          class="text-sm"
        >
          {{
            t(
              "inbox.evidence.brokenInheritance.affectedCount",
              { count: finding.affected.length },
              finding.affected.length,
            )
          }}
        </p>
        <ul
          v-if="finding.affected.length > 0"
          class="list-inside list-disc text-sm"
          data-testid="broken-inheritance-affected-list"
        >
          <li
            v-for="def in finding.affected.slice(0, BROKEN_INHERITANCE_AFFECTED_PREVIEW_LIMIT)"
            :key="`${def.defType}/${def.defName}`"
          >
            <RouterLink
              class="underline"
              :to="inspectRoute(`${def.defType}/${def.defName}`)"
            >
              <code>{{ def.defType }}/{{ def.defName }}</code>
            </RouterLink>
          </li>
        </ul>
        <p
          v-if="finding.affected.length > BROKEN_INHERITANCE_AFFECTED_PREVIEW_LIMIT"
          class="text-text-muted text-xs"
        >
          {{
            t("inbox.evidence.brokenInheritance.andMore", {
              count: finding.affected.length - BROKEN_INHERITANCE_AFFECTED_PREVIEW_LIMIT,
            })
          }}
        </p>
        <p
          v-if="finding.truncated > 0"
          class="text-text-muted text-xs"
        >
          {{
            t(
              "inbox.evidence.brokenInheritance.truncated",
              { count: finding.truncated },
              finding.truncated,
            )
          }}
        </p>
      </template>
      <template v-else-if="finding.kind === 'nearMissModReference'">
        <i18n-t
          :keypath="
            finding.referenceKind === 'findModName'
              ? 'inbox.evidence.nearMissModReference.findModName'
              : 'inbox.evidence.nearMissModReference.mayRequireId'
          "
          tag="p"
        >
          <template #modId>
            {{ modLabel.label(finding.referrer) }}
          </template>
          <template #written>
            <code>{{ finding.written }}</code>
          </template>
          <template #candidateName>
            <code>{{ finding.candidateName }}</code>
          </template>
        </i18n-t>
        <i18n-t
          v-if="finding.file"
          keypath="inbox.evidence.nearMissModReference.writtenIn"
          tag="p"
          class="text-text-muted text-xs"
        >
          <template #file>
            <code>{{ finding.file }}</code>
          </template>
        </i18n-t>
      </template>
      <template v-else-if="finding.kind === 'patchWillFail'">
        <i18n-t
          :keypath="patchWillFailKeypath(finding.cause)"
          tag="p"
        >
          <template #modId>
            {{ modLabel.label(finding.modId) }}
          </template>
          <template #operation>
            <code>{{ finding.operation }}</code>
          </template>
          <template #defKey>
            <code>{{ finding.defKey.defType }}/{{ finding.defKey.defName }}</code>
          </template>
          <template
            v-if="finding.cause.kind === 'removedBy' || finding.cause.kind === 'notYetInjected'"
            #causeModId
          >
            {{ modLabel.label(finding.cause.modId) }}
          </template>
        </i18n-t>
        <i18n-t
          v-if="finding.leafXpath"
          keypath="inbox.evidence.patchWillFail.specificField"
          tag="p"
          class="text-text-muted text-xs"
        >
          <template #leafXpath>
            <code>{{ finding.leafXpath }}</code>
          </template>
        </i18n-t>
      </template>

      <template v-else-if="finding.kind === 'contributesNothing'">
        <p>
          {{
            t("inbox.evidence.contributesNothing.body", { modId: modLabel.label(finding.modId) })
          }}
        </p>
      </template>

      <template v-else-if="finding.kind === 'discardedAddition'">
        <i18n-t
          keypath="inbox.evidence.discardedAddition.sentence"
          tag="p"
        >
          <template #replacer>
            {{ modLabel.label(finding.replacer) }}
          </template>
          <template #adder>
            {{ modLabel.label(finding.adder) }}
          </template>
          <template #path>
            <code>{{ finding.path }}</code>
          </template>
          <template #adderPath>
            <code>{{ finding.adderPath }}</code>
          </template>
        </i18n-t>
      </template>

      <template v-else-if="finding.kind === 'danglingDefReference'">
        <p>
          <i18n-t
            keypath="inbox.evidence.danglingDefReference.sentence"
            tag="span"
          >
            <template #name>
              <code>{{ finding.name }}</code>
            </template>
            <template #cause>
              {{ danglingCauseText(finding.cause, t, modLabel.label) }}
            </template>
          </i18n-t>
          <i18n-t
            v-if="finding.likelySound"
            keypath="inbox.evidence.danglingDefReference.likelySound"
            tag="span"
          >
            <template #soundDef>
              <!-- eslint-disable-next-line vue/no-bare-strings-in-template -- a def type name, never translated, same as any other def type/class name in this app. -->
              <code>SoundDef</code>
            </template>
          </i18n-t>
        </p>
        <p
          v-if="finding.referrers.length > 0"
          class="text-sm"
        >
          {{ t("inbox.evidence.danglingDefReference.referencedFrom") }}
        </p>
        <ul
          v-if="finding.referrers.length > 0"
          class="list-inside list-disc text-sm"
          data-testid="dangling-def-reference-referrers-list"
        >
          <li
            v-for="(referrer, index) in finding.referrers"
            :key="index"
          >
            {{ danglingReferrerText(referrer, modLabel.label) }}
          </li>
        </ul>
        <p
          v-if="finding.truncatedReferrers > 0"
          class="text-text-muted text-xs"
        >
          {{
            t(
              "inbox.evidence.danglingDefReference.truncatedReferrers",
              { count: finding.truncatedReferrers },
              finding.truncatedReferrers,
            )
          }}
        </p>
      </template>
    </section>

    <section
      class="border-border-subtle bg-surface-1 flex flex-col gap-2 rounded-lg border p-3"
      data-testid="suggestion-section"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="font-medium">{{
          tm(describeAction(detail.suggestion.action, modLabel.label, (kind) => tm(edgeKindLabel(kind))))
        }}</span>
        <BaseConfidence :confidence="detail.suggestion.confidence" />
      </div>
      <p class="text-text-muted text-sm">
        {{ rationaleText(detail.suggestion.rationaleCode, t, modLabel.label, locale) }}
      </p>
      <div class="flex gap-2">
        <Button
          :label="t('inbox.suggestion.acceptButton')"
          size="small"
          data-testid="accept-button"
          @click="emit('accept')"
        />
        <Button
          :label="t('inbox.suggestion.ignoreButton')"
          size="small"
          severity="secondary"
          data-testid="ignore-button"
          @click="emit('ignore')"
        />
        <Button
          v-if="detail.hasDecision"
          :label="t('inbox.suggestion.revertButton')"
          size="small"
          severity="danger"
          outlined
          data-testid="revert-button"
          @click="emit('revert')"
        />
      </div>
    </section>

    <section
      v-if="detail.suggestion.alternatives.length > 0"
      data-testid="alternatives-section"
    >
      <h3 class="mb-1 text-sm font-semibold">
        {{ t("inbox.suggestion.alternativesHeading") }}
      </h3>
      <AlternativeList
        :alternatives="detail.suggestion.alternatives"
        @pick="(index) => emit('pickAlternative', index)"
      />
    </section>

    <section>
      <Button
        v-if="!noteOpen"
        :label="t('inbox.suggestion.addNoteButton')"
        size="small"
        text
        data-testid="open-note-button"
        @click="emit('openNote')"
      />
      <DecisionNote
        v-else
        :note="detail.note"
        @save="(note) => emit('saveNote', note)"
        @cancel="emit('closeNote')"
      />
    </section>
  </div>
</template>
