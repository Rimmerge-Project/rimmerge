import type { RouteLocationRaw } from "vue-router";
import { formatList } from "@/i18n/format";
import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import { asDefRef } from "@/types/brands";
import type { ActionDto } from "@/types/generated/ActionDto";
import type { DefKeyDto } from "@/types/generated/DefKeyDto";
import type { EdgeKindDto } from "@/types/generated/EdgeKindDto";
import type { MergeEntryKindDto } from "@/types/generated/MergeEntryKindDto";
import type { MergeModEntryDto } from "@/types/generated/MergeModEntryDto";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";
import { assertNever } from "@/utils/assertNever";

/**
 * The `defType` a texture override's `PreferWinner` alternative's `key`
 * carries — a synthetic `DefKey` standing in for a texture path, never a
 * real def type. Mirrors `rim_resolve::domain::finding::TEXTURE_DEF_TYPE`
 * (the constant `DefKey::synthesize_for_texture` builds every such key
 * with); keep the two in sync.
 */
const TEXTURE_DEF_TYPE = "texture";

/**
 * A one-line, human-readable rendering of a `DefKey` — `<defType>/<defName>`
 * for a real def, or `texture <defName>` for a texture override's
 * synthetic key (see {@link TEXTURE_DEF_TYPE}), so a texture alternative
 * reads as "Prefer X's Things/Wall.png" instead of the literal, meaningless
 * "Prefer X's texture/Things/Wall.png".
 */
export function describeDefKey(key: DefKeyDto): string {
  return key.defType === TEXTURE_DEF_TYPE
    ? `texture ${key.defName}`
    : `${key.defType}/${key.defName}`;
}

/**
 * Renders a 0..=100 progress percentage with at most one decimal:
 * `99.30174825…` -> `"99.3%"`, `100` -> `"100%"` (never `"100.0%"`),
 * `0` -> `"0%"`. Progress arrives as `done / total` over ~1000 per-mod
 * ticks, so the raw float would otherwise print its full expansion.
 *
 * Truncates rather than rounds: `99.96` reads `"99.9%"`, so a bar that
 * still has work left never claims to be finished.
 */
export function formatPercentLabel(percent: number): string {
  const truncated = Math.floor(percent * 10) / 10;
  return `${Number.isInteger(truncated) ? truncated.toFixed(0) : truncated.toFixed(1)}%`;
}

/** Renders a 0..=100 confidence as a percentage string, e.g. `"82%"`. */
export function formatConfidence(confidence: number): string {
  return `${confidence}%`;
}

const KIB = 1024;
const MIB = KIB * 1024;
const GIB = MIB * 1024;

/**
 * `1536` -> `"1.50 KiB"`, `0` -> `"0 B"` — human-readable byte
 * formatting for the `/startup` page's table, matching
 * `apps/cli/src/commands/startup.rs::format_bytes`'s own conventions
 * (`KiB`/`MiB`/`GiB`, 1024-based, not the decimal `KB`/`MB`/`GB`) so the
 * two surfaces agree.
 */
export function formatBytes(bytes: number): string {
  if (bytes >= GIB) {
    return `${(bytes / GIB).toFixed(2)} GiB`;
  }
  if (bytes >= MIB) {
    return `${(bytes / MIB).toFixed(2)} MiB`;
  }
  if (bytes >= KIB) {
    return `${(bytes / KIB).toFixed(2)} KiB`;
  }
  return `${bytes} B`;
}

/**
 * Joins a profile directory and a file name for display only (never for
 * an actual file-system call — the backend does the real joining). Mirrors
 * `apps/desktop/src-tauri/src/dto/settings.rs::apply_report`'s own
 * `profile_dir.join(fileName)` convention so the apply dialog can show the
 * decisions/rules paths before `apply` has actually run.
 */
export function profileFilePath(profileDir: string, fileName: string): string {
  const separator = profileDir.endsWith("/") || profileDir.endsWith("\\") ? "" : "/";
  return `${profileDir}${separator}${fileName}`;
}

/** An id-to-text function that does nothing — {@link describeAction}'s default when a caller doesn't care about display names. */
function identityLabel(id: string): string {
  return id;
}

/** {@link describeAction}'s default for `edgeKindText` — a caller that doesn't pass one gets the raw wire value, same as before this returned a descriptor. */
function identityEdgeKindText(kind: EdgeKindDto): string {
  return kind;
}

/**
 * A one-line, human-readable description of a resolving action,
 * returned as a {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text. `label` (default: the
 * identity function) renders every mod id the description embeds —
 * pass `useModLabel().label` to make the mode toggle apply here too.
 * `edgeKindText` renders `dropEdge`/`keepEdge`'s own edge kind — pass
 * `(kind) => tm(edgeKindLabel(kind))` so it reads as a real label
 * (e.g. "Load after") instead of the raw camelCase wire value; both
 * resolvers are called eagerly, here, since a `MessageDescriptor`'s
 * `params` can only hold plain values, never a nested translatable one.
 */
export function describeAction(
  action: ActionDto,
  label: (id: string) => string = identityLabel,
  edgeKindText: (kind: EdgeKindDto) => string = identityEdgeKindText,
): MessageDescriptor {
  switch (action.kind) {
    case "accept":
      return descriptor("action.accept");
    case "ignore":
      return descriptor("action.ignore");
    case "reorder":
      return descriptor("action.reorder", {
        after: label(action.after),
        before: label(action.before),
      });
    case "preferWinner":
      return descriptor("action.preferWinner", {
        winner: label(action.winner),
        defKey: describeDefKey(action.key),
      });
    case "chooseCandidate":
      return descriptor("action.chooseCandidate", {
        chosen: label(action.chosen),
        after: label(action.after),
      });
    case "dropEdge":
      return descriptor("action.dropEdge", {
        edgeKind: edgeKindText(action.edgeKind),
        after: label(action.after),
        before: label(action.before),
      });
    case "keepEdge":
      return descriptor("action.keepEdge", {
        edgeKind: edgeKindText(action.edgeKind),
        after: label(action.after),
        before: label(action.before),
      });
    case "addTag":
      return descriptor("action.addTag", { modId: label(action.modId), tag: action.tag });
    case "removeTag":
      return descriptor("action.removeTag", { tag: action.tag, modId: label(action.modId) });
    case "excludeFromCluster":
      return descriptor("action.excludeFromCluster", {
        modId: label(action.modId),
        rule: action.rule,
      });
    case "removeMod":
      return descriptor("action.removeMod", { modId: label(action.modId) });
    case "merge":
      return descriptor("action.merge", { defKey: describeDefKey(action.key) });
    case "shipAsset":
      return descriptor("action.shipAsset", {
        from: label(action.from),
        texturePath: action.texturePath,
      });
    case "promoteRule":
      return action.rule.kind === "pair"
        ? descriptor("action.promoteRulePair", {
            after: label(action.rule.after),
            before: label(action.rule.before),
          })
        : descriptor("action.promoteRulePlacement", { modId: label(action.rule.modId) });
    case "dropRule":
      return descriptor("action.dropRule", {
        after: label(action.after),
        before: label(action.before),
      });
    default:
      return assertNever(action);
  }
}

/**
 * The single mod id `action` most centrally names, for a `title` tooltip
 * next to {@link describeAction}'s text — `null` for `accept`/`ignore`
 * (name no mod) and `merge` (names a def, not a mod) and for a
 * multi-id action where no one id is more "the" subject than the other
 * (picking `after` there would be an arbitrary half-answer).
 */
export function primaryModIdOfAction(action: ActionDto): string | null {
  switch (action.kind) {
    case "accept":
    case "ignore":
    case "merge":
      return null;
    case "reorder":
    case "dropEdge":
    case "keepEdge":
      return action.after;
    case "preferWinner":
      return action.winner;
    case "chooseCandidate":
      return action.chosen;
    case "addTag":
    case "removeTag":
    case "excludeFromCluster":
    case "removeMod":
      return action.modId;
    case "shipAsset":
      return action.from;
    case "promoteRule":
      return action.rule.kind === "pair" ? action.rule.after : action.rule.modId;
    case "dropRule":
      return action.after;
    default:
      return assertNever(action);
  }
}

/**
 * The structural guard key:
 * a guarded `needsFieldInput` preview can never become `complete`, however
 * its fields are chosen — the guard forces a confirmation, it doesn't ask
 * for one. Shared verbatim by {@link describeMergeState} (the merge
 * editor's own state pill) and {@link describeSkippedMergeReason} (the
 * apply dialog's skip line), so a guarded def reads identically wherever
 * it shows up rather than "N fields need input" in one place and
 * something else in another.
 */
const STRUCTURAL_GUARD_KEY = "merge.state.structuralGuard";

/**
 * A short, human-readable label for a merge decision's own progress —
 * shared by the inbox's `FindingCard` pill and the merge editor's
 * `MergeHeader` state pill so both read identically (e.g. "1 field needs
 * input", never "1 of N fields need input"). Returns a
 * {@link MessageDescriptor} — render it through `t()`/`useTranslateMessage()`.
 *
 * `structuralGuardField` (default: not guarded) is the structural guard's signal
 * (`MergePreviewDto.structuralGuard?.field`) — when set, a `needsFieldInput`
 * state renders {@link STRUCTURAL_GUARD_KEY} instead of a field count:
 * the guard's own `unresolved`/`total` are both the def's whole field
 * count by construction (`rim-session`'s `state_from_plan`), so "N fields
 * need input" would contradict a totals line reporting zero real
 * conflicts. `FindingCard`'s own call site has no guard field to pass
 * (`ResolutionSummaryDto` doesn't carry one) and keeps the plain,
 * field-count wording.
 */
export function describeMergeState(
  state: MergeStateDto,
  structuralGuardField?: string | null,
): MessageDescriptor {
  switch (state.kind) {
    case "complete":
      return descriptor("merge.state.merged");
    case "needsFieldInput":
      if (structuralGuardField) {
        return descriptor(STRUCTURAL_GUARD_KEY);
      }
      return descriptor("merge.state.needsInput", { count: state.unresolved }, state.unresolved);
    case "cannotMerge":
      return descriptor("merge.state.cannotMerge");
    default:
      return assertNever(state);
  }
}

/**
 * One skipped `Merge`/`ShipAsset` decision's own reason, for the apply
 * dialog's skip list. Returns a {@link MessageDescriptor} for the
 * translatable case, or the raw English `reason` string for a
 * `cannotMerge` entry (an open-ended technical diagnostic, never
 * translated — see `apps/desktop/CLAUDE.md`'s i18n conventions) —
 * callers branch on `typeof` to tell the two apart. `entry` is the
 * matching `get_merge_mod` row, when one was found — `undefined` (no
 * matching entry, e.g. the merge mod wasn't queried, or was queried before
 * this apply ran) falls back to the same plain wording a genuine
 * needs-input skip gets, never a misleading guarded-sounding message.
 *
 * A structurally-guarded def override reads
 * {@link STRUCTURAL_GUARD_KEY} — the same phrase
 * {@link describeMergeState} uses — rather than "still needs input": no
 * field choice can ever complete it, so the honest ask is "decide
 * something other than Merge for this finding", not "finish filling in
 * fields". A `cannotMerge` entry (an unlocatable `ShipAsset`, or a
 * `DefOverride`/`PatchCollision` whose replay hit an unsupported op)
 * names its own reason verbatim, since that text already says
 * specifically what's wrong.
 */
export function describeSkippedMergeReason(
  entry: MergeModEntryDto | undefined,
): MessageDescriptor | string {
  if (entry?.structuralGuardField) {
    return descriptor(STRUCTURAL_GUARD_KEY);
  }
  if (entry?.state.kind === "cannotMerge") {
    return entry.state.reason;
  }
  return descriptor("merge.skipped.stillNeedsInput");
}

/**
 * The apply dialog's own merge-mod summary line, counting the two groups
 * separately — an auto-suggested `patchCollision` merge (mirrors
 * RimWorld's own sequential patch composition) versus an explicitly
 * decided `defOverride` merge (a field-merged def copy no author shipped,
 * entering the merge mod only by the user's own explicit choice) — plus a
 * third, unlabelled clause for a `ShipAsset` decision, so a profile whose
 * only decision is one still gets a summary line under a checkbox whose
 * own label mentions "merge/asset decision".
 *
 * Counts only entries that actually *write* something: `state.kind ===
 * "complete"` *and*
 * `opCount > 0` — a decided zero-op merge (every
 * field already resolves to the winner's own value) still reaches
 * `plans`/renders in `rim-session`'s own `RenderMergeMod`, but contributes
 * no patch file and no real content, so counting it as something this
 * apply "will write" would overclaim. A skipped/guarded entry, or a
 * zero-op one, contributes to no count. `null` when nothing reaches the
 * merge mod at all, so the caller can hide the line entirely rather than
 * rendering an empty sentence.
 *
 * `t` is the caller's own `useI18n().t` (or an equivalent) — this
 * function builds a *composite*, variable-clause sentence (1-3 parts
 * joined through the locale's list conjunction, each independently pluralized), which a single
 * `MessageDescriptor`'s flat `params` can't express; every other helper
 * in this module returns a descriptor instead, this is the one
 * exception, for that reason.
 */
export function describeMergeModGroups(
  entries: readonly MergeModEntryDto[],
  t: (key: string, params?: Record<string, unknown>, count?: number) => string,
  locale: string,
): string | null {
  const writes = entries.filter((entry) => entry.state.kind === "complete" && entry.opCount > 0);
  const patchCollisionCount = writes.filter((entry) => entry.kind === "patchCollision").length;
  const defOverrideCount = writes.filter((entry) => entry.kind === "defOverride").length;
  const assetCount = writes.filter((entry) => entry.kind === "asset").length;
  if (patchCollisionCount === 0 && defOverrideCount === 0 && assetCount === 0) {
    return null;
  }
  const parts: string[] = [];
  if (patchCollisionCount > 0) {
    parts.push(
      t("merge.summary.patchCollision", { count: patchCollisionCount }, patchCollisionCount),
    );
  }
  if (defOverrideCount > 0) {
    parts.push(t("merge.summary.defOverride", { count: defOverrideCount }, defOverrideCount));
  }
  if (assetCount > 0) {
    parts.push(t("merge.summary.asset", { count: assetCount }, assetCount));
  }
  return t("merge.summary.willWrite", { parts: formatList(locale, parts) });
}

/** A short label for a `MergeModEntryDto.kind`, e.g. "patch-collision" — `MergeModEntryList`'s own per-entry chip. */
export function mergeEntryKindLabel(kind: MergeEntryKindDto): MessageDescriptor {
  switch (kind) {
    case "defOverride":
      return descriptor("merge.entryKind.defOverride");
    case "patchCollision":
      return descriptor("merge.entryKind.patchCollision");
    case "asset":
      return descriptor("merge.entryKind.asset");
    default:
      return assertNever(kind);
  }
}

/** The status-token classes matching {@link describeMergeState}'s severity. */
export function mergeStateBadgeClasses(state: MergeStateDto): string {
  switch (state.kind) {
    case "complete":
      return "bg-status-auto-soft text-status-auto";
    case "needsFieldInput":
      return "bg-status-input-soft text-status-input";
    case "cannotMerge":
      return "bg-status-danger-soft text-status-danger";
    default:
      return assertNever(state);
  }
}

/**
 * A leaf value (or a short XML fragment) renders fine through
 * `MergeValueCell` — it truncates a leaf's own text already. A long `li`
 * item's or keyed-map entry's whole XML doesn't: `MergeValueCell` renders
 * it in full inside a scrolling `<pre>`, which is the right call in the
 * merge editor's own field table but too tall for a change summary's
 * compact rows — those fall back to a `title`-tooltipped, truncated
 * `<code>` span instead. Shared by `ChangedFieldList` and
 * `ChangeSummary`'s patch-collision block so both draw the same line
 * between "fits" and "doesn't."
 */
export function fitsMergeValueCell(value: string | null, isXml: boolean): boolean {
  const INLINE_ITEM_MAX = 80;
  return !isXml || value === null || value.length <= INLINE_ITEM_MAX;
}

/**
 * A route to the def page for `defRef`, known non-null — every call site
 * that already has a non-nullable `defRef` (a `DefLinkDto`/`MergePreviewDto`/
 * `DefSearchHitDto` field, or a nullable one already narrowed by a `v-if`
 * on the same element) uses this rather than reaching for
 * {@link inspectHref} and asserting away its `| null` with a bare `!` —
 * the assertion made once, here, instead of at each call site.
 * a route param, never string-concatenated into a path: `vue-router`'s
 * own param encoder already percent-encodes an embedded `/` (e.g.
 * `ThingDef/Wall`) and decodes it back on the other side (see
 * {@link import("@/types/brands").DefRef}'s own doc comment).
 */
export function inspectRoute(defRef: string): RouteLocationRaw {
  return { name: "def", params: { defRef: asDefRef(defRef) } };
}

/**
 * {@link inspectRoute}, or `null` when there isn't a `defRef` at all — for
 * a call site whose own `defRef` field is genuinely nullable and isn't
 * narrowed ahead of time (e.g. rendering a disabled link).
 */
export function inspectHref(defRef: string | null): RouteLocationRaw | null {
  return defRef === null ? null : inspectRoute(defRef);
}
