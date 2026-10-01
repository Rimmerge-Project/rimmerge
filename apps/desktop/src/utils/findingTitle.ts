import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { FindingDto } from "@/types/generated/FindingDto";
import { assertNever } from "@/utils/assertNever";
import { edgeKindLabel } from "@/utils/edgeKind";
import { describeDefKey } from "@/utils/format";
import { placementLabel } from "@/utils/placement";

/** The `t()` shape below takes — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>, count?: number) => string;

/**
 * A short, human title for one finding, built directly from the
 * strongly-typed {@link FindingDto} — never by re-parsing a
 * `FindingKey`'s canonical text. Ported from `src-tauri/src/dto/finding/
 * evidence.rs`'s former `finding_title` (removed there): every mod id
 * now goes through `label` (`useModLabel().label`), a real bug the Rust
 * version had (ids were "baked in once" with no per-viewer name/id
 * toggle).
 *
 * `t`/`label` are the caller's own `useI18n().t`/`useModLabel().label` —
 * this function can't call either composable itself (a plain `utils/`
 * helper, not a component). Returns a {@link MessageDescriptor} —
 * render it through `t()`/`useTranslateMessage()`, never as text.
 */
export function findingTitle(
  finding: FindingDto,
  t: Translate,
  label: (id: string) => string,
): MessageDescriptor {
  switch (finding.kind) {
    case "edgeDropped":
      return finding.winner
        ? descriptor("finding.title.edgeDroppedWithWinner", {
            before: label(finding.before),
            after: label(finding.after),
            winner: label(finding.winner.after),
          })
        : descriptor("finding.title.edgeDroppedNoWinner", {
            before: label(finding.before),
            after: label(finding.after),
          });
    case "declarationQuestioned":
      return descriptor("finding.title.declarationQuestioned", {
        after: label(finding.declaredAfter),
        before: label(finding.declaredBefore),
        relation: t(edgeKindLabel(finding.relationKind).key),
      });
    case "declarationOverridden":
      return descriptor("finding.title.declarationOverridden", {
        after: label(finding.declaredAfter),
        before: label(finding.declaredBefore),
        winner: label(finding.by.after),
      });
    case "anyOfChoice":
      return descriptor(
        "finding.title.anyOfChoice",
        {
          after: label(finding.after),
          count: finding.candidates.length,
          assembly: finding.assembly,
        },
        finding.candidates.length,
      );
    case "defOverride":
    case "patchCollision":
      return descriptor("common.verbatim", { text: describeDefKey(finding.key) });
    case "textureOverride":
      return descriptor("finding.title.textureOverride", { texturePath: finding.texturePath });
    case "duplicateAssembly":
      // The Rust original always said "N mods", ungrammatically, even
      // for N = 1 — fixed here now that this is a real vue-i18n plural
      // message rather than a hand-built string; the coordinator signed
      // off on this one wording improvement during the L5 port.
      return descriptor(
        "finding.title.duplicateAssembly",
        { assembly: finding.assemblyName, count: finding.owners.length },
        finding.owners.length,
      );
    case "duplicateTemplateName":
      return descriptor(
        "finding.title.duplicateTemplateName",
        { name: finding.name, count: finding.owners.length },
        finding.owners.length,
      );
    case "keyedTranslationCollision":
      return descriptor(
        "finding.title.keyedTranslationCollision",
        { a: label(finding.a), b: label(finding.b), count: finding.keys.length },
        finding.keys.length,
      );
    case "soundOverride":
      return descriptor(
        "finding.title.soundOverride",
        { path: finding.path, count: finding.owners.length },
        finding.owners.length,
      );
    case "undeclaredTypeDependency":
      return descriptor("finding.title.undeclaredTypeDependency", {
        user: label(finding.user),
        provider: label(finding.provider),
        typeName: finding.typeName,
      });
    case "runtimePatchCollision":
      return descriptor(
        "finding.title.runtimePatchCollision",
        {
          target: `${finding.targetType}.${finding.targetMethod}`,
          count: finding.owners.length,
        },
        finding.owners.length,
      );
    case "transpilerCollision":
      return descriptor(
        "finding.title.transpilerCollision",
        {
          target: `${finding.targetType}.${finding.targetMethod}`,
          count: finding.owners.length,
        },
        finding.owners.length,
      );
    case "likelyDuplicateMod":
      return descriptor(
        "finding.title.likelyDuplicateMod",
        {
          a: label(finding.a),
          b: label(finding.b),
          count: finding.sharedDefs,
        },
        finding.sharedDefs,
      );
    case "missingMod":
      return descriptor("finding.title.missingMod", { modId: label(finding.modId) });
    case "missingDependency":
      return descriptor("finding.title.missingDependency", {
        modId: label(finding.modId),
        dependency: finding.displayName ?? label(finding.dependency),
      });
    case "incompatiblePair":
      return descriptor("finding.title.incompatiblePair", {
        a: label(finding.a),
        b: label(finding.b),
      });
    case "unsupportedVersion":
      return descriptor("finding.title.unsupportedVersion", { modId: label(finding.modId) });
    case "undeclaredHardDependency":
      return descriptor("finding.title.undeclaredHardDependency", {
        after: label(finding.after),
        before: label(finding.before),
      });
    case "lazyReferenceViolated":
      return descriptor("finding.title.lazyReferenceViolated", {
        after: label(finding.after),
        before: label(finding.before),
      });
    case "tagInferred":
      return descriptor("finding.title.tagInferred", {
        modId: label(finding.modId),
        tag: finding.tag,
      });
    case "ruleOverruled":
      return finding.winner
        ? descriptor("finding.title.ruleOverruledWithWinner", {
            before: label(finding.before),
            after: label(finding.after),
            winner: label(finding.winner.after),
          })
        : descriptor("finding.title.ruleOverruledNoWinner", {
            before: label(finding.before),
            after: label(finding.after),
          });
    case "placementOverruled":
      return descriptor("finding.title.placementOverruled", {
        placement: t(placementLabel(finding.placement).key),
        modId: label(finding.modId),
      });
    case "placementQuestioned":
      return descriptor("finding.title.placementQuestioned", {
        placement: t(placementLabel(finding.placement).key),
        modId: label(finding.modId),
        other: label(finding.other),
      });
    case "placementOrderingOverridden":
      return descriptor("finding.title.placementOrderingOverridden", {
        modId: label(finding.modId),
        pinned: label(finding.pinned),
      });
    case "placementPromotesDependents":
      return descriptor(
        "finding.title.placementPromotesDependents",
        { modId: label(finding.modId), count: finding.promoted.length },
        finding.promoted.length,
      );
    case "missingTexturePath":
      return descriptor("finding.title.missingTexturePath", {
        defKey: describeDefKey(finding.def),
        field: finding.field,
      });
    case "patchWillFail":
      return descriptor("finding.title.patchWillFail", {
        modId: label(finding.modId),
        defKey: describeDefKey(finding.defKey),
      });
    case "contributesNothing":
      return descriptor("finding.title.contributesNothing", { modId: label(finding.modId) });
    case "undecodableTexture":
      return descriptor("finding.title.undecodableTexture", {
        modId: label(finding.modId),
        path: finding.path,
      });
    case "brokenInheritance":
      return descriptor("finding.title.brokenInheritance", {
        modId: label(finding.modId),
        parentName: finding.parentName,
      });
    case "nearMissModReference":
      return descriptor("finding.title.nearMissModReference", {
        referrer: label(finding.referrer),
        written: finding.written,
      });
    case "discardedAddition":
      return descriptor("finding.title.discardedAddition", {
        replacer: label(finding.replacer),
        adder: label(finding.adder),
      });
    case "danglingDefReference":
      return descriptor("finding.title.danglingDefReference", { name: finding.name });
    default:
      return assertNever(finding);
  }
}
