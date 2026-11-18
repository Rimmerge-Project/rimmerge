import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { SourceDto } from "@/types/generated/SourceDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link SourceDto} — `ModDetailPage.vue`'s
 * own header and `ModFilterBar.vue`'s source filter both render this rather
 * than the raw wire value. Exhaustive via {@link assertNever}. Returns a
 * {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function sourceLabel(source: SourceDto): MessageDescriptor {
  switch (source) {
    case "core":
      return descriptor("source.core");
    case "dlc":
      return descriptor("source.dlc");
    case "local":
      return descriptor("source.local");
    case "workshop":
      return descriptor("source.workshop");
    default:
      return assertNever(source);
  }
}

/**
 * The descriptive label the Mods list's source badge shows as tooltip and
 * accessible text ("Steam Workshop", "Local mod", ...). Distinct from
 * {@link sourceLabel}, whose short words suit the filter dropdown and
 * detail headers. Exhaustive via {@link assertNever}.
 */
export function sourceBadgeLabel(source: SourceDto): MessageDescriptor {
  switch (source) {
    case "core":
      return descriptor("mods.table.sourceBadge.core");
    case "dlc":
      return descriptor("mods.table.sourceBadge.dlc");
    case "local":
      return descriptor("mods.table.sourceBadge.local");
    case "workshop":
      return descriptor("mods.table.sourceBadge.workshop");
    default:
      return assertNever(source);
  }
}
