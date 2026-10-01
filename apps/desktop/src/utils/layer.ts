import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { LayerDto } from "@/types/generated/LayerDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link LayerDto} — every load-order
 * edge-strength layer this app can ever receive, so `EdgeChip.vue`
 * never interpolates `edge.layer` raw (camelCase, e.g.
 * `"declaredOverride"`). Exhaustive via {@link assertNever}, the same
 * convention {@link import("@/utils/edgeKind").edgeKindLabel} uses.
 * Returns a {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function layerLabel(layer: LayerDto): MessageDescriptor {
  switch (layer) {
    case "hard":
      return descriptor("layer.hard");
    case "anyOf":
      return descriptor("layer.anyOf");
    case "declaredOverride":
      return descriptor("layer.declaredOverride");
    case "declared":
      return descriptor("layer.declared");
    case "userDecision":
      return descriptor("layer.userDecision");
    case "rimSortUser":
      return descriptor("layer.rimSortUser");
    case "rimSortCommunity":
      return descriptor("layer.rimSortCommunity");
    case "steamDb":
      return descriptor("layer.steamDb");
    case "inferred":
      return descriptor("layer.inferred");
    case "soft":
      return descriptor("layer.soft");
    case "awareness":
      return descriptor("layer.awareness");
    default:
      return assertNever(layer);
  }
}
