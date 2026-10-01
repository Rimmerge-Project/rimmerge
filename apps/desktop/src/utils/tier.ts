import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { TierDto } from "@/types/generated/TierDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link TierDto} — `TierBadge.vue`'s own
 * badge text and `order.tierReason.*`'s own `Tier`/`Source` cases both
 * render this rather than duplicating the label table. Exhaustive via
 * {@link assertNever}. Returns a {@link MessageDescriptor} — render it
 * through `t()`/`useTranslateMessage()`, never as text.
 */
export function tierLabel(tier: TierDto): MessageDescriptor {
  switch (tier) {
    case "core":
      return descriptor("order.tier.core");
    case "dlc":
      return descriptor("order.tier.dlc");
    case "top":
      return descriptor("order.tier.top");
    case "body":
      return descriptor("order.tier.body");
    case "bottom":
      return descriptor("order.tier.bottom");
    default:
      return assertNever(tier);
  }
}
