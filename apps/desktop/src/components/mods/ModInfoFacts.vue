<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import TierBadge from "@/components/order/TierBadge.vue";
import { useModLabel } from "@/composables/useModLabel";
import { formatList } from "@/i18n/format";
import type { ActiveModInfoDto } from "@/types/generated/ActiveModInfoDto";
import type { GeneratedKindDto } from "@/types/generated/GeneratedKindDto";
import type { InactiveModInfoDto } from "@/types/generated/InactiveModInfoDto";
import { assertNever } from "@/utils/assertNever";

const { info } = defineProps<{ info: ActiveModInfoDto | InactiveModInfoDto }>();
const { t, locale } = useI18n();
const router = useRouter();
const modLabel = useModLabel();

const isActive = (value: ActiveModInfoDto | InactiveModInfoDto): value is ActiveModInfoDto =>
  "position" in value;

function openWhyPanel(modId: string): void {
  void router.push({ name: "order-mod", params: { modId } });
}

function openFindings(modId: string): void {
  void router.push({ path: "/inbox", query: { mod: modId } });
}

/** Exhaustive over {@link GeneratedKindDto} so a fourth generated kind is a compile error here, not a silent "assignment" mislabel. */
function generatedKindLabel(kind: GeneratedKindDto): string {
  switch (kind) {
    case "merge":
      return t("modInfo.panel.generatedMerge");
    case "patch":
      return t("modInfo.panel.generatedPatch");
    case "assignment":
      return t("modInfo.panel.generatedAssignment");
    default:
      return assertNever(kind);
  }
}

/**
 * One whole sentence for the cost line, not the three `{{ t(x) }}: {{ v }}`
 * fragments the template used to concatenate — each header's own
 * `startup.*` translation is threaded in as a named param (the "already
 * translated fragment as a value" pattern, not the forbidden
 * concatenate-around-a-value one), so a translator can still reorder or
 * repunctuate the whole line freely.
 */
function costLine(cost: NonNullable<ActiveModInfoDto["cost"]>): string {
  return t("modInfo.panel.costLine", {
    defsLabel: t("startup.defsHeader"),
    defCount: cost.defCount,
    patchOpsLabel: t("startup.patchOpsHeader"),
    patchOps: cost.patchOps,
    dllsLabel: t("startup.dllsHeader"),
    assemblyCount: cost.assemblyCount,
  });
}
</script>

<template>
  <div
    class="flex flex-col gap-4"
    data-testid="mod-info-facts"
  >
    <section data-testid="mod-info-facts-root">
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.rootFolderLabel") }}
      </h3>
      <p class="text-text-muted font-mono text-xs break-all">
        {{ info.root || "." }}
      </p>
    </section>

    <section
      v-if="info.supportedVersions.length > 0"
      data-testid="mod-info-facts-supported-versions"
    >
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.supportedVersionsLabel") }}
      </h3>
      <p class="text-text-muted text-sm">
        {{ formatList(locale, info.supportedVersions) }}
      </p>
    </section>

    <section
      v-if="isActive(info)"
      data-testid="mod-info-facts-placement"
    >
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.positionLabel") }}
      </h3>
      <p class="flex items-center gap-2 text-sm">
        <span>#{{ info.position + 1 }}</span>
        <TierBadge :tier="info.tier" />
        <button
          type="button"
          class="text-accent cursor-pointer text-xs underline"
          data-testid="mod-info-why-here"
          @click="openWhyPanel(info.modId)"
        >
          {{ t("modInfo.panel.whyHereLink") }}
        </button>
      </p>
      <p
        v-if="info.previousPosition !== null"
        class="text-text-faint text-xs"
      >
        {{ t("modInfo.panel.previousPositionLine", { position: info.previousPosition + 1 }) }}
      </p>
    </section>

    <section
      v-if="isActive(info)"
      data-testid="mod-info-facts-cost"
    >
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.costLabel") }}
      </h3>
      <p
        v-if="info.cost"
        class="text-text-muted text-sm"
      >
        {{ costLine(info.cost) }}
      </p>
      <RouterLink
        to="/startup"
        class="text-accent text-xs underline"
        data-testid="mod-info-see-startup"
      >
        {{ t("modInfo.panel.seeStartupLink") }}
      </RouterLink>
    </section>

    <section data-testid="mod-info-facts-tags">
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.tagsLabel") }}
      </h3>
      <div
        v-if="isActive(info) && info.tags.length > 0"
        class="flex flex-wrap gap-1"
      >
        <span
          v-for="tag in info.tags"
          :key="tag"
          class="bg-surface-2 text-text-muted rounded px-2 py-0.5 text-xs"
        >{{ tag }}</span>
      </div>
      <p
        v-else
        class="text-text-muted text-sm"
      >
        {{ t("mods.edges.none") }}
      </p>
    </section>

    <section
      v-if="isActive(info)"
      data-testid="mod-info-facts-dependents"
    >
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.requiredByHeading") }}
      </h3>
      <ul class="text-text-muted flex flex-col gap-0.5 text-sm">
        <li>{{ t("modInfo.panel.hardDependents", { count: info.hardDependents }, info.hardDependents) }}</li>
        <li>{{ t("modInfo.panel.softDependents", { count: info.softDependents }, info.softDependents) }}</li>
        <li>
          {{
            t(
              "modInfo.panel.awarenessDependents",
              { count: info.awarenessDependents },
              info.awarenessDependents,
            )
          }}
        </li>
      </ul>
      <p
        v-if="info.isFrameworkCandidate"
        class="text-text-faint mt-1 text-xs"
        data-testid="mod-info-framework-candidate"
      >
        {{ t("modInfo.panel.frameworkCandidate") }}
      </p>
    </section>

    <section
      v-if="isActive(info)"
      data-testid="mod-info-facts-findings"
    >
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("mods.detail.findingsHeading") }}
      </h3>
      <button
        type="button"
        class="text-accent cursor-pointer text-sm underline"
        data-testid="mod-info-findings-link"
        @click="openFindings(info.modId)"
      >
        {{ t("modInfo.panel.findingsCount", { count: info.findingsTotal }, info.findingsTotal) }}
      </button>
      <p
        v-if="info.needsInputCount > 0"
        class="text-status-danger text-xs"
      >
        {{ t("modInfo.panel.needsInputCount", { count: info.needsInputCount }, info.needsInputCount) }}
      </p>
    </section>

    <section
      v-if="info.generated"
      data-testid="mod-info-facts-generated"
    >
      <p class="text-text-muted text-sm">
        {{ t("modInfo.panel.generatedLine", { kind: generatedKindLabel(info.generated.kind) }) }}
      </p>
    </section>

    <section
      v-if="isActive(info) && info.loadedFolders.length > 0"
      data-testid="mod-info-facts-loaded-folders"
    >
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.loadedFoldersLabel") }}
      </h3>
      <ul class="text-text-muted flex flex-col gap-0.5 text-xs">
        <li
          v-for="folder in info.loadedFolders"
          :key="folder"
          class="font-mono break-all"
        >
          {{ folder || "." }}
        </li>
      </ul>
    </section>

    <section data-testid="mod-info-facts-declared">
      <h3 class="text-text mb-1 text-sm font-semibold">
        {{ t("modInfo.panel.declaredHeading") }}
      </h3>
      <dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
        <dt class="text-text-muted">
          {{ t("mods.detail.loadAfterLabel") }}
        </dt>
        <dd>{{ formatList(locale, info.declared.loadAfter.map(modLabel.label)) || "—" }}</dd>
        <dt class="text-text-muted">
          {{ t("mods.detail.loadBeforeLabel") }}
        </dt>
        <dd>{{ formatList(locale, info.declared.loadBefore.map(modLabel.label)) || "—" }}</dd>
        <dt class="text-text-muted">
          {{ t("mods.detail.forceLoadAfterLabel") }}
        </dt>
        <dd>{{ formatList(locale, info.declared.forceLoadAfter.map(modLabel.label)) || "—" }}</dd>
        <dt class="text-text-muted">
          {{ t("mods.detail.forceLoadBeforeLabel") }}
        </dt>
        <dd>{{ formatList(locale, info.declared.forceLoadBefore.map(modLabel.label)) || "—" }}</dd>
        <dt class="text-text-muted">
          {{ t("mods.detail.dependenciesLabel") }}
        </dt>
        <dd>
          {{
            formatList(
              locale,
              info.declared.dependencies.map((dependency) =>
                modLabel.labelWithFallback(dependency.id, dependency.displayName),
              ),
            ) || "—"
          }}
        </dd>
        <dt class="text-text-muted">
          {{ t("mods.detail.incompatibleWithLabel") }}
        </dt>
        <dd>{{ formatList(locale, info.declared.incompatibleWith.map(modLabel.label)) || "—" }}</dd>
      </dl>
    </section>
  </div>
</template>
