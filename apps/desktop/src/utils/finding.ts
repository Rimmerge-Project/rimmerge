import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { DefKeyDto } from "@/types/generated/DefKeyDto";
import type { FindingDto } from "@/types/generated/FindingDto";
import type { FindingKindDto } from "@/types/generated/FindingKindDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A representative mod id for `finding` — the one to jump to when the
 * user asks to "open in order view" / "open mod detail" from the inbox.
 * Every finding kind names at least one mod; this picks the most
 * relevant single one (the dependent side of an edge, the winner of an
 * override, the first owner of a shared payload, ...).
 */
export function primaryModIdOf(finding: FindingDto): string {
  switch (finding.kind) {
    case "edgeDropped":
    case "undeclaredHardDependency":
    case "lazyReferenceViolated":
    case "anyOfChoice":
      return finding.after;
    case "declarationQuestioned":
    case "declarationOverridden":
      return finding.declaredAfter;
    case "defOverride":
      return finding.winner;
    case "patchCollision":
      return finding.mods[0] ?? "";
    case "textureOverride":
      return finding.owners[0] ?? "";
    case "duplicateAssembly":
    case "duplicateTemplateName":
    case "soundOverride":
      return finding.owners[0] ?? "";
    case "keyedTranslationCollision":
    case "likelyDuplicateMod":
    case "incompatiblePair":
      return finding.a;
    case "runtimePatchCollision":
    case "transpilerCollision":
      return finding.owners[0] ?? "";
    case "undeclaredTypeDependency":
      return finding.user;
    case "missingMod":
    case "unsupportedVersion":
      return finding.modId;
    case "missingDependency":
      return finding.modId;
    case "tagInferred":
      return finding.modId;
    case "ruleOverruled":
      return finding.after;
    case "placementOverruled":
    case "placementQuestioned":
    case "placementOrderingOverridden":
    case "placementPromotesDependents":
      return finding.modId;
    case "missingTexturePath":
      return finding.referrer;
    case "patchWillFail":
      return finding.modId;
    case "contributesNothing":
      return finding.modId;
    case "undecodableTexture":
      return finding.modId;
    case "brokenInheritance":
      return finding.modId;
    case "nearMissModReference":
      return finding.referrer;
    case "discardedAddition":
      return finding.replacer;
    case "danglingDefReference":
      // No mod is part of this finding's own identity (the referrers are
      // evidence, not part of the key) — the first referrer's own mod is
      // the closest thing to a "primary" mod, falling back to empty when
      // somehow there are none.
      return finding.referrers[0]?.referrer.modId ?? "";
    default:
      return assertNever(finding);
  }
}

/**
 * The contested def a finding names, for the `m` shortcut and the
 * `merge` alternative — only `defOverride` and `patchCollision` findings
 * can be merged. `null` for
 * every other kind.
 */
export function mergeableDefKeyOf(finding: FindingDto): DefKeyDto | null {
  switch (finding.kind) {
    case "defOverride":
    case "patchCollision":
      return finding.key;
    default:
      return null;
  }
}

/**
 * A short, human-readable label for one finding kind, for filter chips.
 * Returns a {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function findingKindLabel(kind: FindingKindDto): MessageDescriptor {
  switch (kind) {
    case "edgeDropped":
      return descriptor("finding.kind.edgeDropped");
    case "anyOfChoice":
      return descriptor("finding.kind.anyOfChoice");
    case "defOverride":
      return descriptor("finding.kind.defOverride");
    case "patchCollision":
      return descriptor("finding.kind.patchCollision");
    case "textureOverride":
      return descriptor("finding.kind.textureOverride");
    case "duplicateAssembly":
      return descriptor("finding.kind.duplicateAssembly");
    case "likelyDuplicateMod":
      return descriptor("finding.kind.likelyDuplicateMod");
    case "missingMod":
      return descriptor("finding.kind.missingMod");
    case "missingDependency":
      return descriptor("finding.kind.missingDependency");
    case "incompatiblePair":
      return descriptor("finding.kind.incompatiblePair");
    case "unsupportedVersion":
      return descriptor("finding.kind.unsupportedVersion");
    case "undeclaredHardDependency":
      return descriptor("finding.kind.undeclaredHardDependency");
    case "lazyReferenceViolated":
      return descriptor("finding.kind.lazyReferenceViolated");
    case "declarationQuestioned":
      return descriptor("finding.kind.declarationQuestioned");
    case "declarationOverridden":
      return descriptor("finding.kind.declarationOverridden");
    case "duplicateTemplateName":
      return descriptor("finding.kind.duplicateTemplateName");
    case "keyedTranslationCollision":
      return descriptor("finding.kind.keyedTranslationCollision");
    case "soundOverride":
      return descriptor("finding.kind.soundOverride");
    case "undeclaredTypeDependency":
      return descriptor("finding.kind.undeclaredTypeDependency");
    case "runtimePatchCollision":
      return descriptor("finding.kind.runtimePatchCollision");
    case "transpilerCollision":
      return descriptor("finding.kind.transpilerCollision");
    case "tagInferred":
      return descriptor("finding.kind.tagInferred");
    case "ruleOverruled":
      return descriptor("finding.kind.ruleOverruled");
    case "placementOverruled":
      return descriptor("finding.kind.placementOverruled");
    case "placementQuestioned":
      return descriptor("finding.kind.placementQuestioned");
    case "placementOrderingOverridden":
      return descriptor("finding.kind.placementOrderingOverridden");
    case "placementPromotesDependents":
      return descriptor("finding.kind.placementPromotesDependents");
    case "missingTexturePath":
      return descriptor("finding.kind.missingTexturePath");
    case "patchWillFail":
      return descriptor("finding.kind.patchWillFail");
    case "contributesNothing":
      return descriptor("finding.kind.contributesNothing");
    case "undecodableTexture":
      return descriptor("finding.kind.undecodableTexture");
    case "brokenInheritance":
      return descriptor("finding.kind.brokenInheritance");
    case "nearMissModReference":
      return descriptor("finding.kind.nearMissModReference");
    case "discardedAddition":
      return descriptor("finding.kind.discardedAddition");
    case "danglingDefReference":
      return descriptor("finding.kind.danglingDefReference");
    default:
      return assertNever(kind);
  }
}

/** Every finding kind, in the fixed order they're offered as filter chips. */
export const ALL_FINDING_KINDS: readonly FindingKindDto[] = [
  "edgeDropped",
  "anyOfChoice",
  "defOverride",
  "patchCollision",
  "textureOverride",
  "duplicateAssembly",
  "likelyDuplicateMod",
  "missingMod",
  "missingDependency",
  "incompatiblePair",
  "unsupportedVersion",
  "undeclaredHardDependency",
  "lazyReferenceViolated",
  "declarationQuestioned",
  "declarationOverridden",
  "duplicateTemplateName",
  "keyedTranslationCollision",
  "soundOverride",
  "undeclaredTypeDependency",
  "runtimePatchCollision",
  "transpilerCollision",
  "tagInferred",
  "ruleOverruled",
  "placementOverruled",
  "placementQuestioned",
  "placementOrderingOverridden",
  "placementPromotesDependents",
  "missingTexturePath",
  "patchWillFail",
  "contributesNothing",
  "undecodableTexture",
  "brokenInheritance",
  "nearMissModReference",
  "discardedAddition",
  "danglingDefReference",
];

/**
 * `key`'s canonical `snake_case` kind prefix (e.g. `"def_override"`)
 * converted to the `camelCase` `FindingKindDto` it names, or `null` when
 * it doesn't match one — defensive only, since every prefix
 * `FindingKey`'s `Display` impl emits has an entry in
 * {@link ALL_FINDING_KINDS}.
 */
function findingKindFromKeyPrefix(prefix: string): FindingKindDto | null {
  const camelCase = prefix.replace(/_([a-z])/g, (_match, letter: string) => letter.toUpperCase());
  return (ALL_FINDING_KINDS as readonly string[]).includes(camelCase)
    ? (camelCase as FindingKindDto)
    : null;
}

/**
 * A thin fallback naming the kind a `ResolutionSummaryDto`'s canonical
 * `key` text belongs to — the kind's own label
 * ({@link findingKindLabel}), or the raw key text when the prefix doesn't
 * match a known kind. Never re-parses the key's per-kind fields (a def
 * key, a texture path, a runtime-patch target's owner count, ...) — that
 * evidence comes from the backend as
 * `ResolutionSummaryDto.title`/`ResolutionDetailDto.title`, built
 * straight from the typed `Finding`, not by splitting the key's `:`-
 * separated fields back apart client-side. Kept for callers that only
 * have the bare key string, not a full resolution DTO to read `title`
 * off.
 */
export function describeFindingKey(key: string): MessageDescriptor {
  const [prefix = ""] = key.split(":");
  const kind = findingKindFromKeyPrefix(prefix);
  // `common.verbatim` (`"{text}"`) is the one passthrough key for a
  // value that isn't itself translatable (here, the raw key text when
  // the prefix doesn't match a known kind) but still needs to reach a
  // `MessageDescriptor` consumer.
  return kind ? findingKindLabel(kind) : descriptor("common.verbatim", { text: key });
}

/**
 * Whether `key` names one of the finding kinds whose `Accept` suggestion
 * carries no data of its own — `ledger::suggest` always recommends a bare
 * `Action::Accept` for `runtimePatchCollision`/`ruleOverruled`/
 * `placementOverruled`/`placementQuestioned` (there's no single action
 * that "fixes" an owner-set collision or a disclosure finding), so
 * `describeAction`'s title for an undecided card in the profile inbox
 * would otherwise just read "Accept" for every one of them.
 * `FindingCard` reaches for {@link describeFindingKey} instead while the
 * effective action is still that bare `Accept` — not only inside a
 * scoped patch's own inbox, which already has its own reason to prefer
 * it (see that function's doc comment).
 *
 * `patch_will_fail` joins this list too: every one of
 * its four causes suggests a bare `Action::Accept` (`ledger::suggest::
 * patch_will_fail`) — there's no single action that "fixes" a predicted
 * patch failure either, only a `Reorder` *alternative* for the two
 * order-fixable causes.
 *
 * `contributes_nothing` joins it too: a bare
 * `Action::Accept` with no alternatives at all (`ledger::suggest::
 * contributes_nothing`) — disabling the mod is the user's own act in
 * their mod manager, not an action this engine offers.
 *
 * `transpiler_collision` joins it too: a bare
 * `Action::Accept` with no alternatives at all (`ledger::suggest::
 * transpiler_collision`) — deliberately so, since neither this engine nor
 * the sorter can say which transpiler should run first.
 *
 * The declared-edge override's own `declaration_overridden` joins it too:
 * `ledger::suggest::declaration_overridden`
 * always recommends a bare `Action::Accept` with no alternatives, the
 * identical disclosure-only shape the kinds above already have — without
 * this, every undecided card for it would just read "Accept".
 *
 * `broken_inheritance` and `near_miss_mod_reference` join it too: both
 * are always a bare `Action::Accept` with no alternatives
 * (`ledger::suggest::defs::broken_inheritance`/
 * `ledger::suggest::mods::near_miss_mod_reference`) — the fix in either
 * case is the referencing mod's own authoring, outside this workspace.
 *
 * `undecodable_texture` joins it too: a bare `Action::Accept` with no
 * alternatives (`ledger::suggest::assets::undecodable_texture`) — the
 * fix is re-encoding the file, outside this workspace. A pre-existing
 * gap this batch's own `findings.spec.ts` coverage caught: it never had
 * a dedicated e2e fixture row before, so nothing exercised its card text
 * reading "Accept" instead of its real title.
 *
 * `dangling_def_reference` joins it too: a bare `Action::Accept` with no
 * alternatives (`ledger::suggest::defs::dangling_def_reference`) —
 * cross-references resolve after every mod's defs and patches have
 * loaded, so there is no order-fixable alternative to offer, and the fix
 * itself is the referencing mod's own authoring, outside this workspace.
 */
export function describesFindingWhenAccepted(key: string): boolean {
  const [prefix = ""] = key.split(":");
  return (
    prefix === "runtime_patch_collision" ||
    prefix === "transpiler_collision" ||
    prefix === "rule_overruled" ||
    prefix === "placement_overruled" ||
    prefix === "placement_questioned" ||
    prefix === "contributes_nothing" ||
    prefix === "patch_will_fail" ||
    prefix === "declaration_overridden" ||
    prefix === "broken_inheritance" ||
    prefix === "near_miss_mod_reference" ||
    prefix === "undecodable_texture" ||
    prefix === "discarded_addition" ||
    prefix === "dangling_def_reference"
  );
}
