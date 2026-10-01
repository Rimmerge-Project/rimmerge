import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import en from "@/locales/en.json";
import type { EdgeDto } from "@/types/generated/EdgeDto";
import type { TierReasonDto } from "@/types/generated/TierReasonDto";
import { edgeProvenanceDetail, tierReasonText } from "@/utils/provenance";

const { t } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
}).global;

const label = (id: string): string => `Label(${id})`;

function edge(overrides: Partial<EdgeDto>): EdgeDto {
  return {
    after: "a.mod",
    before: "b.mod",
    layer: "declared",
    kind: null,
    provenance: { kind: "engine" },
    detail: null,
    ...overrides,
  };
}

describe("edgeProvenanceDetail", () => {
  it("returns the raw analyzer detail verbatim for an engine edge", () => {
    expect(
      edgeProvenanceDetail(
        edge({
          kind: "loadAfter",
          provenance: { kind: "engine" },
          detail: "a.mod declares loadAfter b.mod",
        }),
        t,
      ),
    ).toBe("a.mod declares loadAfter b.mod");
  });

  it("names the shared assembly for an anyOf edge", () => {
    expect(
      edgeProvenanceDetail(edge({ provenance: { kind: "anyOf", assembly: "Shared.dll" } }), t),
    ).toBe("any-of candidate for Shared.dll");
  });

  it("names the rule's origin, with no comment", () => {
    expect(
      edgeProvenanceDetail(
        edge({ provenance: { kind: "rule", origin: "rimSortCommunity", comment: null } }),
        t,
      ),
    ).toBe("rule (RimSort community)");
  });

  it("appends the rule's own comment when present", () => {
    expect(
      edgeProvenanceDetail(
        edge({
          provenance: { kind: "rule", origin: "userDecision", comment: "load order testing" },
        }),
        t,
      ),
    ).toBe("rule (User decision): load order testing");
  });

  it("names the tier for a tier boundary edge", () => {
    expect(edgeProvenanceDetail(edge({ provenance: { kind: "tier", tier: "top" } }), t)).toBe(
      "tier boundary (Top)",
    );
  });
});

describe("tierReasonText", () => {
  it("names the source", () => {
    const reason: TierReasonDto = { kind: "source", source: "core" };
    expect(tierReasonText(reason, t, label)).toBe("source: Core");
  });

  it("names the placement rule's origin", () => {
    const reason: TierReasonDto = { kind: "placement", origin: "rimSortUser" };
    expect(tierReasonText(reason, t, label)).toBe("placement rule (RimSort user)");
  });

  it("names both mods of the promoting edge through the label resolver", () => {
    const reason: TierReasonDto = {
      kind: "promotedBy",
      edge: edge({ before: "b.mod", after: "a.mod" }),
    };
    expect(tierReasonText(reason, t, label)).toBe("promoted by Label(b.mod) → Label(a.mod)");
  });

  it("names no source or placement signal for body", () => {
    const reason: TierReasonDto = { kind: "body" };
    expect(tierReasonText(reason, t, label)).toBe("no source or placement signal");
  });
});
