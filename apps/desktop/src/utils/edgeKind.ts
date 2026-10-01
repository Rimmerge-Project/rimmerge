import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { EdgeKindDto } from "@/types/generated/EdgeKindDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link EdgeKindDto} — every edge kind
 * this app can ever receive, so `ModEdges.vue`/`EdgeChip.vue` never
 * interpolate `edge.kind` raw (camelCase, e.g. `"patchRemovedNode"`).
 * Exhaustive via {@link assertNever}, the same convention
 * {@link findingKindLabel} uses. Returns a {@link MessageDescriptor} —
 * render it through `t()`/`useTranslateMessage()`, never as text.
 */
export function edgeKindLabel(kind: EdgeKindDto): MessageDescriptor {
  switch (kind) {
    case "assemblyRef":
      return descriptor("edgeKind.assemblyRef");
    case "forceLoadAfter":
      return descriptor("edgeKind.forceLoadAfter");
    case "forceLoadBefore":
      return descriptor("edgeKind.forceLoadBefore");
    case "loadAfter":
      return descriptor("edgeKind.loadAfter");
    case "loadBefore":
      return descriptor("edgeKind.loadBefore");
    case "modDependency":
      return descriptor("edgeKind.modDependency");
    case "findMod":
      return descriptor("edgeKind.findMod");
    case "ifModActive":
      return descriptor("edgeKind.ifModActive");
    case "patchTargetsDef":
      return descriptor("edgeKind.patchTargetsDef");
    case "mayRequire":
      return descriptor("edgeKind.mayRequire");
    case "patchInjectedNode":
      return descriptor("edgeKind.patchInjectedNode");
    case "assemblyVersionPrecedence":
      return descriptor("edgeKind.assemblyVersionPrecedence");
    case "usesType":
      return descriptor("edgeKind.usesType");
    case "parentTemplate":
      return descriptor("edgeKind.parentTemplate");
    case "patchRemovedNode":
      return descriptor("edgeKind.patchRemovedNode");
    case "retextureAfterOwner":
      return descriptor("edgeKind.retextureAfterOwner");
    case "defOverrideAfterOrigin":
      return descriptor("edgeKind.defOverrideAfterOrigin");
    case "patchSelectsInjectedNode":
      return descriptor("edgeKind.patchSelectsInjectedNode");
    case "patchInvalidatesPredicate":
      return descriptor("edgeKind.patchInvalidatesPredicate");
    case "patchRemovedNodeCosmetic":
      return descriptor("edgeKind.patchRemovedNodeCosmetic");
    case "replaceDiscardsAddition":
      return descriptor("edgeKind.replaceDiscardsAddition");
    default:
      return assertNever(kind);
  }
}
