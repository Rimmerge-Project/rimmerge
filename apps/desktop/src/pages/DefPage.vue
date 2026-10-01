<script setup lang="ts">
import { computed, onMounted, onScopeDispose, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import BaseStatusPill from "@/components/base/BaseStatusPill.vue";
import XmlPreview from "@/components/merge/XmlPreview.vue";
import DefSearchBox from "@/components/mods/DefSearchBox.vue";
import { isInert } from "@/composables/shortcutTargets";
import { useModLabel } from "@/composables/useModLabel";
import { useOrderSource } from "@/composables/useOrderSource";
import { usePagedRows } from "@/composables/usePagedRows";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { useDefInspectionQuery } from "@/queries/defs";
import { asDefRef, type DefRef } from "@/types/brands";
import { asRecord } from "@/types/dtoMaps";
import type { EffectiveFieldFilterDto } from "@/types/generated/EffectiveFieldFilterDto";
import type { PatcherDto } from "@/types/generated/PatcherDto";
import type { ProvenanceDto } from "@/types/generated/ProvenanceDto";
import type { TemplateAmbiguityDto } from "@/types/generated/TemplateAmbiguityDto";
import { caveatLabel } from "@/utils/caveat";
import { describeFindingKey } from "@/utils/finding";
import { describeFindModGate } from "@/utils/findModGate";
import { inspectRoute } from "@/utils/format";
import { orderSourceLabel, orderSourceSentenceLabel } from "@/utils/orderSource";

/** Matches `rim_session::MAX_PAGE_SIZE` — the server caps `filter.limit` to this. */
const PAGE_SIZE = 200;

/**
 * The three visually-distinct provenance buckets a field row can carry
 * (`inherited`/`unattributedTemplate` share one look — see
 * {@link provenanceClass}) — a small legend next to the effective-def
 * table so the distinction never rides on color alone. `labelKey` is a
 * literal `en.json` key, resolved by `t()` at render time (never a
 * label baked in once here) so a locale switch updates it.
 */
const PROVENANCE_LEGEND: ReadonlyArray<{ kind: string; labelKey: string; class: string }> = [
  {
    kind: "owner",
    labelKey: "defPage.provenanceLegendOwner",
    class: "bg-surface-2 text-text-muted",
  },
  {
    kind: "patch",
    labelKey: "defPage.provenanceLegendPatched",
    class: "bg-status-input/20 text-status-input",
  },
  {
    kind: "inherited",
    labelKey: "defPage.provenanceLegendInherited",
    class: "bg-accent/10 text-accent",
  },
];

const { t, locale } = useI18n();
const route = useRoute();
const router = useRouter();
const modLabel = useModLabel();
const tm = useTranslateMessage();
const order = useOrderSource();

const defRef = computed<DefRef | null>(() => {
  const param = route.params["defRef"];
  return typeof param === "string" && param.length > 0 ? asDefRef(param) : null;
});

const onlyPatched = ref(false);
const fieldOffset = ref(0);

const filter = computed<EffectiveFieldFilterDto>(() => ({
  onlyPatched: onlyPatched.value,
  offset: fieldOffset.value,
  limit: PAGE_SIZE,
}));

const query = useDefInspectionQuery(defRef, filter);

// A different def, or toggling `onlyPatched`, starts the field list over
// from page one — see `usePagedRows` for how pages are kept (by offset,
// not a plain append) so an invalidation refetch of the current page
// never duplicates it.
const fieldsPage = usePagedRows(
  fieldOffset,
  computed(() =>
    query.data.value
      ? { items: query.data.value.fields, total: query.data.value.fieldsTotal }
      : null,
  ),
  () => [defRef.value, onlyPatched.value],
);

const expandedPatchers = ref<Set<string>>(new Set());
function toggleExpanded(modId: string): void {
  const next = new Set(expandedPatchers.value);
  if (next.has(modId)) {
    next.delete(modId);
  } else {
    next.add(modId);
  }
  expandedPatchers.value = next;
}
function isExpanded(patcher: PatcherDto): boolean {
  return expandedPatchers.value.has(patcher.modId);
}

/**
 * `replayError`/`reached` together distinguish three real states of the
 * full, load-ordered fold (`DefInspection.effective`'s own account, never
 * a standalone per-mod replay): cleanly replayed, the fold's own stopper,
 * or never reached at all (a *different*, earlier mod stopped the fold
 * first) — the last case must not read as a false "replays cleanly".
 */
function patcherStatusLabel(patcher: PatcherDto): string {
  if (!patcher.reached) {
    return t("defPage.patcherStatusNotReached");
  }
  return patcher.replayError
    ? t("defPage.patcherStatusReplayFailed")
    : t("defPage.patcherStatusReplaysCleanly");
}

function patcherStatusClass(patcher: PatcherDto): string {
  if (!patcher.reached) {
    return "text-text-faint";
  }
  return patcher.replayError ? "text-status-danger" : "text-status-auto";
}

/** A short, human label for one field's provenance pill. */
function provenanceLabel(provenance: ProvenanceDto): string {
  switch (provenance.kind) {
    case "owner":
      return t("defPage.provenanceOwner", { mod: modLabel.label(provenance.modId) });
    case "patch":
      return t("defPage.provenancePatch", { mod: modLabel.label(provenance.modId) });
    case "inherited":
      return t("defPage.provenanceInherited", {
        defType: provenance.template.defType,
        defName: provenance.template.defName,
        owner: modLabel.label(provenance.owner),
      });
    case "unattributedTemplate":
      return t("defPage.provenanceUnattributedTemplate", {
        defType: provenance.template.defType,
        defName: provenance.template.defName,
      });
  }
}

function provenanceClass(provenance: ProvenanceDto): string {
  switch (provenance.kind) {
    case "owner":
      return "bg-surface-2 text-text-muted";
    case "patch":
      return "bg-status-input/20 text-status-input";
    case "inherited":
    case "unattributedTemplate":
      return "bg-accent/10 text-accent";
  }
}

/**
 * Groups a {@link TemplateAmbiguityDto}'s flat `resolutions` map by which
 * registrant each known child actually resolves to, in the ambiguity's
 * own `registrants` (load) order — mirrors `apps/cli`'s own
 * `by_resolution` grouping in `print_inspection_text` exactly, so the two
 * surfaces read the same way for the same def.
 */
function ambiguityByRegistrant(
  ambiguity: TemplateAmbiguityDto,
): ReadonlyArray<{ registrant: string; children: readonly string[] }> {
  const byResolution = new Map<string, string[]>();
  for (const [child, resolvedTo] of Object.entries(asRecord(ambiguity.resolutions))) {
    const children = byResolution.get(resolvedTo) ?? [];
    children.push(child);
    byResolution.set(resolvedTo, children);
  }
  return ambiguity.registrants.map((registrant) => ({
    registrant,
    children: byResolution.get(registrant) ?? [],
  }));
}

/** `stopped_at`'s own human-readable reason, for the `Partial` banner. */
const stoppedReason = computed(() => {
  const completeness = query.data.value?.completeness;
  if (completeness?.kind !== "partial") {
    return null;
  }
  const stopper = completeness.stoppedAt;
  if (stopper.kind === "replay") {
    return t("defPage.replayStopper", {
      mod: modLabel.label(stopper.modId),
      opIndex: stopper.opIndex,
      error: stopper.error,
    });
  }
  return stopper.reason;
});

/** `Esc`: back to wherever this page was reached from — the same inert-target rules every other page's shortcuts use. */
function handleKeydown(event: KeyboardEvent): void {
  if (isInert(event.target)) {
    return;
  }
  if (event.key === "Escape") {
    router.back();
  }
}

onMounted(() => window.addEventListener("keydown", handleKeydown));
onScopeDispose(() => window.removeEventListener("keydown", handleKeydown));
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <p
      v-if="query.isPending.value"
      class="text-text-muted text-sm"
      data-testid="def-page-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="query.error.value"
      class="text-status-danger text-sm"
      data-testid="def-page-error"
    >
      {{ query.error.value instanceof Error ? query.error.value.message : t("defPage.loadFailed") }}
    </p>

    <template v-else-if="query.data.value">
      <header class="flex flex-col gap-2">
        <div class="flex items-center justify-between gap-2">
          <h1
            class="text-text font-mono text-lg font-semibold"
            data-testid="def-page-ref"
          >
            {{ query.data.value.defRef }}
          </h1>
          <button
            type="button"
            class="border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text cursor-pointer rounded-full border px-2 py-0.5 text-xs disabled:cursor-not-allowed"
            data-testid="def-page-source"
            :disabled="order.isSwitching.value"
            :title="t('defPage.sourceTitle', { source: tm(orderSourceSentenceLabel(query.data.value.source)) })"
            @click="order.select(query.data.value.source === 'current' ? 'suggested' : 'current')"
          >
            {{ tm(orderSourceLabel(query.data.value.source)) }}
          </button>
        </div>
        <DefSearchBox />
        <ul
          v-if="query.data.value.findings.length > 0"
          class="flex flex-wrap gap-1"
          data-testid="def-page-findings"
        >
          <li
            v-for="finding in query.data.value.findings"
            :key="finding.key"
          >
            <RouterLink
              :to="{ path: '/inbox', query: { search: finding.key } }"
              :title="finding.key"
              class="flex items-center gap-1"
            >
              <BaseStatusPill :status="finding.status" />
              <span class="text-text-muted text-xs">{{ tm(describeFindingKey(finding.key)) }}</span>
            </RouterLink>
          </li>
        </ul>
      </header>

      <p
        v-if="stoppedReason"
        class="border-status-input bg-status-input/10 text-status-input rounded border p-2 text-sm"
        data-testid="def-page-partial-banner"
      >
        {{ t("defPage.partialBanner", { reason: stoppedReason }) }}
      </p>

      <div
        v-if="query.data.value.templateAmbiguity"
        class="border-status-input bg-status-input/10 text-status-input flex flex-col gap-2 rounded border p-2 text-sm"
        data-testid="def-page-template-ambiguity"
      >
        <p>
          {{
            t(
              "defPage.ambiguousTemplateIntro",
              { count: query.data.value.templateAmbiguity.registrants.length },
              query.data.value.templateAmbiguity.registrants.length,
            )
          }}
        </p>
        <ul class="flex flex-col gap-1 text-xs">
          <li
            v-for="group in ambiguityByRegistrant(query.data.value.templateAmbiguity)"
            :key="group.registrant"
          >
            <span class="font-medium">{{ modLabel.label(group.registrant) }}</span>
            — {{ t("defPage.knownChildrenResolve", { count: group.children.length }, group.children.length) }}
            <span v-if="group.children.length > 0">
              ({{ formatList(locale, group.children.map((child) => modLabel.label(child))) }})
            </span>
          </li>
        </ul>
      </div>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("defPage.ownersHeading") }}
        </h2>
        <ul
          class="flex flex-wrap gap-2"
          data-testid="def-page-owners"
        >
          <li
            v-for="owner in query.data.value.owners"
            :key="owner.modId"
            class="border-border-subtle text-text-muted rounded border px-2 py-0.5 text-xs"
            :class="owner.modId === query.data.value.winner ? 'border-accent text-accent font-medium' : ''"
          >
            <span class="text-text-faint">#{{ owner.position }}</span>
            {{ modLabel.label(owner.modId) }}
            <span
              v-if="owner.modId === query.data.value.winner && !query.data.value.templateAmbiguity"
              class="text-text-faint"
              data-testid="def-page-winner"
            >{{ t("defPage.winnerTag") }}</span>
            <span
              v-if="owner.modId === query.data.value.winner && query.data.value.templateAmbiguity"
              class="text-text-faint"
              :title="t('defPage.representativeTitle')"
              data-testid="def-page-winner"
            >{{ t("defPage.representativeTag") }}</span>
            <span
              v-if="owner.isGenerated"
              class="text-text-faint"
            >{{ t("defPage.generatedTag") }}</span>
          </li>
        </ul>
      </section>

      <section v-if="query.data.value.patchers.length > 0">
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("defPage.patchersHeading") }}
        </h2>
        <ul
          class="flex flex-col gap-2"
          data-testid="def-page-patchers"
        >
          <li
            v-for="patcher in query.data.value.patchers"
            :key="patcher.modId"
            class="border-border-subtle rounded border p-2"
          >
            <button
              type="button"
              class="flex w-full cursor-pointer items-center justify-between gap-2 text-left text-sm"
              :data-testid="`def-page-patcher-toggle-${patcher.modId}`"
              @click="toggleExpanded(patcher.modId)"
            >
              <span>
                {{ modLabel.label(patcher.modId) }}
                <span
                  v-if="patcher.isGenerated"
                  class="text-text-faint text-xs"
                >{{ t("defPage.generatedTag") }}</span>
              </span>
              <span
                class="text-xs"
                :class="patcherStatusClass(patcher)"
              >
                {{ patcherStatusLabel(patcher) }}
              </span>
            </button>
            <ul
              v-if="isExpanded(patcher)"
              class="mt-2 flex flex-col gap-1 text-xs"
              :data-testid="`def-page-patcher-ops-${patcher.modId}`"
            >
              <li
                v-for="(op, index) in patcher.ops"
                :key="index"
                class="border-border-subtle border-t pt-1"
              >
                <div class="font-mono">
                  {{ op.class }}
                  <span
                    v-if="op.isWrapped"
                    class="text-text-faint"
                    :title="t('defPage.wrappedTitle')"
                  >{{ t("defPage.wrappedTag") }}</span>
                </div>
                <div
                  v-if="op.xpath"
                  class="text-text-muted break-all"
                >
                  {{ op.xpath }}
                </div>
                <div
                  v-if="op.findModContext.length > 0"
                  class="text-text-faint"
                >
                  {{
                    t("defPage.gatesPrefix", {
                      gates: formatList(
                        locale,
                        op.findModContext.map((gate) => tm(describeFindModGate(gate, locale))),
                      ),
                    })
                  }}
                </div>
              </li>
              <li
                v-if="patcher.replayError"
                class="text-status-danger"
              >
                {{ patcher.replayError }}
              </li>
              <li
                v-for="(caveat, index) in patcher.caveats"
                :key="index"
                class="text-status-input"
              >
                {{ caveatLabel(caveat, t, modLabel.label, locale) }}
              </li>
            </ul>
          </li>
        </ul>
      </section>

      <section v-if="query.data.value.parents.length > 0">
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("defPage.parentsHeading") }}
        </h2>
        <ul
          class="flex flex-wrap gap-2"
          data-testid="def-page-parents"
        >
          <li
            v-for="parent in query.data.value.parents"
            :key="parent.defRef"
          >
            <RouterLink
              :to="inspectRoute(parent.defRef)"
              class="text-accent text-xs underline"
            >
              {{ parent.defRef }}
            </RouterLink>
            <span class="text-text-faint text-xs"> ({{ modLabel.label(parent.owner) }})</span>
          </li>
        </ul>
      </section>

      <section v-if="query.data.value.children.length > 0">
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("defPage.childrenHeading") }}
        </h2>
        <ul
          class="flex flex-wrap gap-2"
          data-testid="def-page-children"
        >
          <li
            v-for="child in query.data.value.children"
            :key="child.defRef"
          >
            <RouterLink
              :to="inspectRoute(child.defRef)"
              class="text-accent text-xs underline"
            >
              {{ child.defRef }}
            </RouterLink>
            <span class="text-text-faint text-xs"> ({{ modLabel.label(child.owner) }})</span>
          </li>
        </ul>
      </section>

      <section>
        <div class="mb-1 flex items-center justify-between">
          <h2 class="text-text text-base font-semibold">
            {{ t("defPage.effectiveDefHeading") }}
          </h2>
          <label class="text-text-muted flex items-center gap-1 text-xs">
            <input
              v-model="onlyPatched"
              type="checkbox"
              data-testid="def-page-only-patched"
            >
            {{ t("defPage.onlyPatchedLabel") }}
          </label>
        </div>
        <ul
          class="mb-2 flex flex-wrap gap-2 text-xs"
          data-testid="def-page-provenance-legend"
        >
          <li
            v-for="entry in PROVENANCE_LEGEND"
            :key="entry.kind"
          >
            <span
              class="rounded px-1.5 py-0.5"
              :class="entry.class"
            >{{ t(entry.labelKey) }}</span>
          </li>
        </ul>
        <table
          class="w-full text-left text-xs"
          data-testid="def-page-fields"
        >
          <thead>
            <tr class="text-text-faint uppercase">
              <th class="py-1 font-medium">
                {{ t("defPage.fields.pathHeader") }}
              </th>
              <th class="py-1 font-medium">
                {{ t("defPage.fields.valueHeader") }}
              </th>
              <th class="py-1 font-medium">
                {{ t("defPage.fields.provenanceHeader") }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="field in fieldsPage.rows.value"
              :key="field.path"
              class="border-border-subtle border-t"
            >
              <td
                class="py-1 font-mono"
                :style="{ paddingLeft: `${field.depth * 12}px` }"
              >
                {{ field.path }}
              </td>
              <td class="max-w-xs truncate py-1 font-mono">
                {{ field.value ?? "—" }}
              </td>
              <td class="py-1">
                <span
                  class="rounded px-1.5 py-0.5"
                  :class="provenanceClass(field.provenance)"
                >
                  {{ provenanceLabel(field.provenance) }}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
        <button
          v-if="fieldsPage.hasMore.value"
          type="button"
          class="border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text mt-2 cursor-pointer self-start rounded border px-2 py-1 text-xs"
          data-testid="def-page-fields-show-more"
          @click="fieldsPage.loadMore"
        >
          {{
            t("defPage.fields.showMore", {
              loaded: fieldsPage.rows.value.length,
              total: fieldsPage.total.value,
            })
          }}
        </button>
      </section>

      <details data-testid="def-page-resolved-xml">
        <summary class="text-text cursor-pointer text-sm font-semibold">
          {{ t("defPage.resolvedXmlSummary") }}
        </summary>
        <div class="mt-2">
          <XmlPreview :content="query.data.value.resolvedXml" />
        </div>
      </details>
    </template>
  </div>
</template>
