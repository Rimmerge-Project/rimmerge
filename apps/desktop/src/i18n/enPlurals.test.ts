import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import { buildPluralRules } from "@/i18n/format";
import en from "@/locales/en.json";

const { t } = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: false,
  messages: { en },
  pluralRules: buildPluralRules(),
}).global;

/**
 * Count messages that used to hold one fixed form ("{count} mods",
 * "child(ren)", a trailing "bytes") and now pick their form from the count
 * they show, the way every call site passes it.
 */
describe("count messages pick their English form from their own count", () => {
  it.each([
    ["mods.page.totalMods", {}, 1, "1 mod"],
    ["mods.page.totalMods", {}, 4, "4 mods"],
    ["merge.header.totalsFields", {}, 1, "1 field"],
    ["merge.header.totalsFields", {}, 2, "2 fields"],
    ["merge.header.totalsConflicts", {}, 1, "1 conflict"],
    ["merge.header.totalsConflicts", {}, 0, "0 conflicts"],
    ["rules.databases.stale", {}, 1, "Stale (1 day)"],
    ["rules.databases.stale", {}, 47, "Stale (47 days)"],
    ["defPage.knownChildrenResolve", {}, 1, "1 known child resolves here"],
    ["defPage.knownChildrenResolve", {}, 3, "3 known children resolve here"],
    ["inbox.textureTile.sizeLine", { format: "PNG" }, 1, "PNG · 1 byte"],
    ["inbox.textureTile.sizeLine", { format: "PNG" }, 2048, "PNG · 2048 bytes"],
    ["inbox.fieldRowsTable.groupHeader", { path: "p" }, 1, "p (1 entry)"],
    ["inbox.fieldRowsTable.groupHeader", { path: "p" }, 3, "p (3 entries)"],
    [
      "finding.title.likelyDuplicateMod",
      { a: "A", b: "B" },
      1,
      "A and B look like duplicates (1 shared def)",
    ],
    [
      "finding.title.likelyDuplicateMod",
      { a: "A", b: "B" },
      12,
      "A and B look like duplicates (12 shared defs)",
    ],
    [
      "finding.title.anyOfChoice",
      { after: "A", assembly: "X" },
      2,
      "A needs one of 2 mods providing X",
    ],
    [
      "apply.diff.counterfactualRejectedLine",
      {},
      1,
      "1 reorder rejected for breaking another operation",
    ],
    [
      "apply.diff.counterfactualRejectedLine",
      {},
      2,
      "2 reorders rejected for breaking another operation",
    ],
    [
      "apply.diff.counterfactualDemotedLine",
      {},
      1,
      '1 "dead target" row turned out to be order-fixable',
    ],
    [
      "mods.page.unresolvableDependency",
      { mod: "M", deps: "D" },
      1,
      "M declares a dependency on D, which is not on disk.",
    ],
    [
      "mods.page.unresolvableDependency",
      { mod: "M", deps: "D and E" },
      2,
      "M declares dependencies on D and E, which are not on disk.",
    ],
    ["apply.diff.counterfactualResolvedLine", {}, 1, "1 failure explained"],
    ["apply.diff.counterfactualResolvedLine", {}, 3, "3 failures explained"],
    ["apply.diff.counterfactualSkippedLine", {}, 1, "1 operation skipped"],
    ["apply.diff.counterfactualSkippedLine", {}, 2, "2 operations skipped"],
    ["gameLog.gaps.neverResumed", {}, 1, "1 logging stop has no resume message after it"],
    ["gameLog.gaps.neverResumed", {}, 2, "2 logging stops have no resume message after them"],
  ] as const)("%s with %s count %d reads %j", (key, params, count, expected) => {
    expect(t(key, { ...params, count }, count)).toContain(expected);
  });

  it("names the dependents of a mod in both forms, with the count in each", () => {
    expect(t("mods.page.dependentsStillActive", { count: 1, mod: "M", deps: "D" }, 1)).toBe(
      "1 active mod still depends on M: D.",
    );
    expect(t("mods.page.dependentsStillActive", { count: 3, mod: "M", deps: "D" }, 3)).toBe(
      "3 active mods still depend on M: D.",
    );
  });

  it("names the owner count of a duplicate template in both forms", () => {
    expect(t("rationale.duplicateTemplateNameExplanation", { ownerCount: 1 }, 1)).toContain(
      "1 mod registers this template name",
    );
    expect(t("rationale.duplicateTemplateNameExplanation", { ownerCount: 5 }, 5)).toContain(
      "5 mods register this template name",
    );
  });

  it("leaves no placeholder unfilled in the plain-English merge-mod summary sentences", () => {
    const without = t("merge.mod.summary", { contents: t("merge.mod.nothingYet") });
    const withSources = t(
      "merge.mod.summaryWithSources",
      { contents: "2 merged defs", count: 2 },
      2,
    );

    expect(without).not.toMatch(/[{}]/);
    expect(withSources).toContain("currently 2 merged defs, sourced from 2 mods.");
  });

  it("reads the empty merge mod as 'currently empty'", () => {
    const empty = t("merge.mod.summary", { contents: t("merge.mod.nothingYet") });

    expect(empty).toContain("— currently empty.");
  });

  it("names the order source in lowercase inside the verify summary sentence", () => {
    const summary = t(
      "apply.diff.summary",
      { count: 1, source: t("shell.orderSource.inSentenceCurrent"), list: "L" },
      1,
    );

    expect(summary).toBe("Checked 1 def against the current order — L.");
  });
});
