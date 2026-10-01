import type { EdgeDto } from "@/types/generated/EdgeDto";
import type { TierReasonDto } from "@/types/generated/TierReasonDto";
import { assertNever } from "@/utils/assertNever";
import { ruleOriginLabel } from "@/utils/rule";
import { sourceLabel } from "@/utils/source";
import { tierLabel } from "@/utils/tier";

/** The `t()` shape below takes — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>) => string;

/**
 * A one-line description of where `edge` came from — `EdgeChip.vue`'s
 * own tooltip. For `EdgeProvenanceDto.kind === "engine"`, this is
 * `edge.detail` verbatim (the analyzer's own English evidence text, kept
 * exactly as ported from the Rust `describe_provenance` it replaces);
 * every other kind is built from `edge.provenance`'s own structured
 * fields through `order.provenance.*`. `t` is the caller's own
 * `useI18n().t` — this function can't call `useI18n()` itself (a plain
 * `utils/` helper, not a component).
 */
export function edgeProvenanceDetail(edge: EdgeDto, t: Translate): string {
  switch (edge.provenance.kind) {
    case "engine":
      return edge.detail ?? "";
    case "anyOf":
      return t("order.provenance.anyOf", { assembly: edge.provenance.assembly });
    case "rule": {
      const origin = t(ruleOriginLabel(edge.provenance.origin).key);
      return edge.provenance.comment
        ? t("order.provenance.ruleWithComment", { origin, comment: edge.provenance.comment })
        : t("order.provenance.rule", { origin });
    }
    case "tier":
      return t("order.provenance.tier", { tier: t(tierLabel(edge.provenance.tier).key) });
    default:
      return assertNever(edge.provenance);
  }
}

/**
 * A one-line description of why a mod sits in its assigned tier —
 * `WhyPanel.vue`'s own position section. `label` is `useModLabel().label`,
 * since `promotedBy` names the two mods of the edge that promoted this
 * one out of its nominal tier. `t`/`label` are the caller's own
 * `useI18n().t`/`useModLabel().label` — this function can't call either
 * composable itself (a plain `utils/` helper, not a component).
 */
export function tierReasonText(
  reason: TierReasonDto,
  t: Translate,
  label: (id: string) => string,
): string {
  switch (reason.kind) {
    case "source":
      return t("order.tierReason.source", { source: t(sourceLabel(reason.source).key) });
    case "placement":
      return t("order.tierReason.placement", { origin: t(ruleOriginLabel(reason.origin).key) });
    case "promotedBy":
      return t("order.tierReason.promotedBy", {
        before: label(reason.edge.before),
        after: label(reason.edge.after),
      });
    case "body":
      return t("order.tierReason.body");
    default:
      return assertNever(reason);
  }
}
