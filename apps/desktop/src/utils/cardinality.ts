import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { CardinalityDto } from "@/types/generated/CardinalityDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link CardinalityDto} —
 * `SchemaTable.vue`'s own cardinality column. Exhaustive via
 * {@link assertNever}. Returns a {@link MessageDescriptor} — render it
 * through `t()`/`useTranslateMessage()`, never as text.
 */
export function cardinalityLabel(cardinality: CardinalityDto): MessageDescriptor {
  switch (cardinality) {
    case "scalar":
      return descriptor("cardinality.scalar");
    case "list":
      return descriptor("cardinality.list");
    default:
      return assertNever(cardinality);
  }
}
