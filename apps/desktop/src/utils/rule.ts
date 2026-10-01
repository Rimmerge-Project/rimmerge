import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { RuleOriginDto } from "@/types/generated/RuleOriginDto";
import { assertNever } from "@/utils/assertNever";

/**
 * Whether `origin` names one of RimSort's three imported database files —
 * never a user's own decision, including a promoted copy. Mirrors
 * `rim_session::settings::is_imported_origin`.
 */
export function isImportedOrigin(origin: RuleOriginDto): boolean {
  return origin !== "userDecision";
}

/**
 * A human-readable label for one {@link RuleOriginDto} — `PairRuleTable.vue`'s
 * own origin column. Reuses the identically-worded `layer.*` keys
 * (`RuleOrigin`'s four variants are a subset of `Layer`'s own — the same
 * underlying concept) rather than duplicating the English text.
 * Exhaustive via {@link assertNever}. Returns a {@link MessageDescriptor}
 * — render it through `t()`/`useTranslateMessage()`, never as text.
 */
export function ruleOriginLabel(origin: RuleOriginDto): MessageDescriptor {
  switch (origin) {
    case "userDecision":
      return descriptor("layer.userDecision");
    case "rimSortUser":
      return descriptor("layer.rimSortUser");
    case "rimSortCommunity":
      return descriptor("layer.rimSortCommunity");
    case "steamDb":
      return descriptor("layer.steamDb");
    default:
      return assertNever(origin);
  }
}
