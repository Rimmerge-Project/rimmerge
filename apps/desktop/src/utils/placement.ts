import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { PlacementDto } from "@/types/generated/PlacementDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link PlacementDto} — `RuleTable.vue`'s
 * own placement column. Exhaustive via {@link assertNever}. Returns a
 * {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function placementLabel(placement: PlacementDto): MessageDescriptor {
  switch (placement) {
    case "top":
      return descriptor("placement.top");
    case "bottom":
      return descriptor("placement.bottom");
    default:
      return assertNever(placement);
  }
}
