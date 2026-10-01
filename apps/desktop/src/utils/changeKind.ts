import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { ChangeKindDto } from "@/types/generated/ChangeKindDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link ChangeKindDto} — `ModChanges.vue`'s
 * own filter chips and table cells. Exhaustive via {@link assertNever},
 * the same convention {@link import("@/utils/edgeKind").edgeKindLabel}
 * uses. Returns a {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function changeKindLabel(kind: ChangeKindDto): MessageDescriptor {
  switch (kind) {
    case "ownsDef":
      return descriptor("changeKind.ownsDef");
    case "ownsTemplate":
      return descriptor("changeKind.ownsTemplate");
    case "patchesDef":
      return descriptor("changeKind.patchesDef");
    case "overridesTexture":
      return descriptor("changeKind.overridesTexture");
    case "overridesSound":
      return descriptor("changeKind.overridesSound");
    case "overridesKeyedTranslation":
      return descriptor("changeKind.overridesKeyedTranslation");
    default:
      return assertNever(kind);
  }
}

/** Every {@link ChangeKindDto} value, in the display order `ModChanges.vue`'s own filter chips use. */
export const CHANGE_KIND_ORDER: readonly ChangeKindDto[] = [
  "ownsDef",
  "ownsTemplate",
  "patchesDef",
  "overridesTexture",
  "overridesSound",
  "overridesKeyedTranslation",
];
