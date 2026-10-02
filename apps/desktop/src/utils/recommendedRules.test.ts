import { describe, expect, it } from "vitest";

import type { ImportReportDto } from "@/types/generated/ImportReportDto";
import type { RecommendedRulesProgressEventDto } from "@/types/generated/RecommendedRulesProgressEventDto";
import type { RecommendedRulesReportDto } from "@/types/generated/RecommendedRulesReportDto";
import type { RuleDatabaseRefreshResultDto } from "@/types/generated/RuleDatabaseRefreshResultDto";
import type { SourceNeedEntryDto } from "@/types/generated/SourceNeedEntryDto";
import {
  describeRecommendedRulesProgress,
  describeRecommendedRulesReport,
  describeSourceNeed,
  describeUnavailable,
  isSteamDownload,
  recommendedRulesButtonLabel,
  recommendedRulesReportSeverity,
  recommendedRulesReportTechnicalDetail,
  rulesClickLabel,
  skippedButtonLabel,
} from "@/utils/recommendedRules";

const FAILURE = { cause: { kind: "transport" }, detail: "connection timed out" } as const;
const IMPORT_REPORT: ImportReportDto = {
  userRules: null,
  communityRules: 4,
  steamDependencies: 9,
  skippedInactiveRules: 0,
  skippedInactiveSteam: 0,
};

const updated = (database: "community" | "steam"): RuleDatabaseRefreshResultDto => ({
  database,
  outcome: { kind: "updated", sha256: "abc", bytes: 10 },
});
const failed = (database: "community" | "steam"): RuleDatabaseRefreshResultDto => ({
  database,
  outcome: { kind: "failed", failure: FAILURE },
});

function report(
  importStep: RecommendedRulesReportDto["import"],
  downloads: RuleDatabaseRefreshResultDto[] = [updated("community"), updated("steam")],
): RecommendedRulesReportDto {
  return { downloads, import: importStep };
}

const imported: RecommendedRulesReportDto["import"] = {
  kind: "imported",
  databases: ["community", "steam"],
  report: IMPORT_REPORT,
};

describe("describeRecommendedRulesReport", () => {
  it("says everything was imported when every download worked", () => {
    expect(describeRecommendedRulesReport(report(imported)).key).toBe(
      "dashboard.guide.getRules.resultDone",
    );
  });

  it("says what downloaded was imported when one download failed", () => {
    const partial = report(imported, [updated("community"), failed("steam")]);

    expect(describeRecommendedRulesReport(partial).key).toBe(
      "dashboard.guide.getRules.resultPartial",
    );
  });

  it("says nothing was downloaded when no import was needed and a download failed", () => {
    const nothing = report({ kind: "notNeeded" }, [failed("community"), failed("steam")]);

    expect(describeRecommendedRulesReport(nothing).key).toBe(
      "dashboard.guide.getRules.resultDownloadFailed",
    );
  });

  it("says there was nothing to do when an already-done rerun downloaded nothing", () => {
    expect(describeRecommendedRulesReport(report({ kind: "notNeeded" }, [])).key).toBe(
      "dashboard.guide.getRules.resultNothingToDo",
    );
  });

  it("reads a no-import run whose downloads all worked as done", () => {
    expect(
      describeRecommendedRulesReport(report({ kind: "notNeeded" }, [updated("community")])).key,
    ).toBe("dashboard.guide.getRules.resultDone");
  });

  it("treats a skipped download as not downloaded", () => {
    const skipped: RuleDatabaseRefreshResultDto = {
      database: "steam",
      outcome: { kind: "skipped", reason: "sourceDisabled" },
    };

    expect(describeRecommendedRulesReport(report({ kind: "notNeeded" }, [skipped])).key).toBe(
      "dashboard.guide.getRules.resultDownloadFailed",
    );
    expect(recommendedRulesReportSeverity(report({ kind: "notNeeded" }, [skipped]))).toBe("error");
  });

  it.each([
    ["importerFailed", "resultImportFailedReadingFile"],
    ["savingRulesFailed", "resultImportFailedSaving"],
  ] as const)("words a %s import failure as its own sentence", (code, suffix) => {
    const failedImport = report({ kind: "failed", code, message: "rules.json is read-only" });

    expect(describeRecommendedRulesReport(failedImport)).toEqual({
      key: `dashboard.guide.getRules.${suffix}`,
    });
  });

  it("carries the backend's English text only as the technical detail", () => {
    const failedImport = report({
      kind: "failed",
      code: "savingRulesFailed",
      message: "rules.json is read-only",
    });

    expect(recommendedRulesReportTechnicalDetail(failedImport)).toBe("rules.json is read-only");
    expect(recommendedRulesReportTechnicalDetail(report(imported))).toBeNull();
  });

  it("tells the user to click again when the import record could not be saved", () => {
    const unrecorded = report({
      kind: "importedManifestNotRecorded",
      databases: ["community", "steam"],
    });

    expect(describeRecommendedRulesReport(unrecorded).key).toBe(
      "dashboard.guide.getRules.resultManifestNotRecorded",
    );
  });

  it("says nothing was imported when another profile was opened meanwhile", () => {
    expect(describeRecommendedRulesReport(report({ kind: "profileChanged" })).key).toBe(
      "dashboard.guide.getRules.resultProfileChanged",
    );
  });
});

describe("recommendedRulesReportSeverity", () => {
  it.each([
    ["all good", report(imported), "success"],
    ["partial", report(imported, [updated("community"), failed("steam")]), "warn"],
    ["nothing downloaded", report({ kind: "notNeeded" }, [failed("steam")]), "error"],
    ["nothing to do", report({ kind: "notNeeded" }, []), "info"],
    ["record not saved", report({ kind: "importedManifestNotRecorded", databases: [] }), "warn"],
    ["import failed", report({ kind: "failed", code: "importerFailed", message: "x" }), "error"],
    ["profile changed", report({ kind: "profileChanged" }), "warn"],
  ] as const)("is %s for %s", (_name, input, expected) => {
    expect(recommendedRulesReportSeverity(input)).toBe(expected);
  });
});

describe("recommendedRulesButtonLabel", () => {
  const community = (need: SourceNeedEntryDto["need"]): SourceNeedEntryDto => ({
    database: "community",
    need,
  });
  const steam = (need: SourceNeedEntryDto["need"]): SourceNeedEntryDto => ({
    database: "steam",
    need,
  });
  const download = { kind: "download", lastFailure: null } as const;
  const failedDownload = { kind: "download", lastFailure: FAILURE } as const;
  const importOnly = { kind: "import" } as const;

  it.each([
    [
      "only community needs the network (no size)",
      [community(download), steam(importOnly)],
      "downloadButton",
    ],
    [
      "steam needs a download (the size)",
      [community(importOnly), steam(download)],
      "downloadButtonWithSteam",
    ],
    ["steam is turned off (the size)", [steam({ kind: "turnOn" })], "downloadButtonWithSteam"],
    [
      "both need downloads (the size)",
      [community(download), steam(download)],
      "downloadButtonWithSteam",
    ],
    [
      "a download failed last time (try again)",
      [community(failedDownload), steam(importOnly)],
      "retryButton",
    ],
    [
      "steam failed last time (try again, the size)",
      [steam(failedDownload)],
      "retryButtonWithSteam",
    ],
    ["only imports are left (import)", [community(importOnly), steam(importOnly)], "importButton"],
  ] as const)("when %s", (_name, sources, suffix) => {
    expect(recommendedRulesButtonLabel(sources).key).toBe(`dashboard.guide.getRules.${suffix}`);
  });
});

describe("skippedButtonLabel and rulesClickLabel", () => {
  const steamOff: SourceNeedEntryDto = { database: "steam", need: { kind: "turnOn" } };
  const communityImport: SourceNeedEntryDto = { database: "community", need: { kind: "import" } };

  it("shows the size when the click would turn on and download Steam", () => {
    expect(skippedButtonLabel([communityImport, steamOff]).key).toBe(
      "dashboard.guide.getRules.getNowButtonWithSteam",
    );
  });

  it("omits the size when only an import is left", () => {
    expect(skippedButtonLabel([communityImport]).key).toBe("dashboard.guide.getRules.getNowButton");
  });

  it("labels a skipped step with Get them now and an offered one with its download wording", () => {
    expect(rulesClickLabel({ kind: "skipped", sources: [steamOff] })?.key).toBe(
      "dashboard.guide.getRules.getNowButtonWithSteam",
    );
    expect(rulesClickLabel({ kind: "needsAction", sources: [steamOff] })?.key).toBe(
      "dashboard.guide.getRules.downloadButtonWithSteam",
    );
  });

  it("offers no click label when nothing is offered", () => {
    expect(rulesClickLabel({ kind: "done", importedRulesInUse: true })).toBeNull();
    expect(rulesClickLabel({ kind: "inProgress" })).toBeNull();
    expect(rulesClickLabel({ kind: "unavailable", reason: "networkOff" })).toBeNull();
  });
});

describe("describeSourceNeed", () => {
  it("words each need as its own sentence", () => {
    const keys = (
      [
        { database: "community", need: { kind: "turnOn" } },
        { database: "community", need: { kind: "download", lastFailure: null } },
        { database: "community", need: { kind: "import" } },
      ] as const
    ).map((entry) => describeSourceNeed(entry, "en").key);

    expect(keys).toEqual([
      "dashboard.guide.getRules.sourceTurnOn",
      "dashboard.guide.getRules.sourceDownload",
      "dashboard.guide.getRules.sourceImport",
    ]);
  });

  it("carries the last failure's headline when a download failed before", () => {
    const line = describeSourceNeed(
      { database: "steam", need: { kind: "download", lastFailure: FAILURE } },
      "en",
    );

    expect(line.key).toBe("dashboard.guide.getRules.sourceRetry");
    expect(line.params).toMatchObject({
      source: { key: "rules.databases.steamLabel" },
      reason: { key: "fetchFailure.transportHeadline" },
    });
  });
});

describe("describeRecommendedRulesProgress", () => {
  it("names the source being downloaded", () => {
    expect(
      describeRecommendedRulesProgress({ kind: "downloading", database: "steam" }),
    ).toMatchObject({
      key: "dashboard.guide.getRules.downloading",
      params: { source: { key: "rules.databases.steamLabel" } },
    });
  });

  it("says importing for the import tick", () => {
    expect(
      describeRecommendedRulesProgress({ kind: "importing", databases: ["community"] }).key,
    ).toBe("dashboard.guide.getRules.importing");
  });
});

describe("isSteamDownload", () => {
  it.each<[RecommendedRulesProgressEventDto | null, boolean]>([
    [null, false],
    [{ kind: "downloading", database: "community" }, false],
    [{ kind: "downloading", database: "steam" }, true],
    [{ kind: "importing", databases: ["steam"] }, false],
  ])("is %j -> %s", (progress, expected) => {
    expect(isSteamDownload(progress)).toBe(expected);
  });
});

describe("describeUnavailable", () => {
  it("links to Settings only when internet access is off", () => {
    expect(describeUnavailable("networkOff")).toMatchObject({
      message: { key: "dashboard.guide.getRules.networkOff" },
      linksToSettings: true,
    });
    expect(describeUnavailable("awaitingFirstRun")).toMatchObject({
      message: { key: "dashboard.guide.getRules.awaitingFirstRun" },
      linksToSettings: false,
    });
  });
});
