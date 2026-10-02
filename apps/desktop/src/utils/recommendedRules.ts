import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { ImportFailureCodeDto } from "@/types/generated/ImportFailureCodeDto";
import type { RecommendedRulesProgressEventDto } from "@/types/generated/RecommendedRulesProgressEventDto";
import type { RecommendedRulesReportDto } from "@/types/generated/RecommendedRulesReportDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import type { SourceNeedDto } from "@/types/generated/SourceNeedDto";
import type { SourceNeedEntryDto } from "@/types/generated/SourceNeedEntryDto";
import type { UnavailableReasonDto } from "@/types/generated/UnavailableReasonDto";
import { assertNever } from "@/utils/assertNever";
import { describeFetchFailure } from "@/utils/fetchFailure";
import { DATABASE_LABEL_KEYS } from "@/utils/ruleDatabases";

/** Whether acting on `need` goes over the network (a toggle to turn on, or a download). */
function needsNetwork(need: SourceNeedDto): boolean {
  switch (need.kind) {
    case "turnOn":
    case "download":
      return true;
    case "import":
      return false;
    default:
      return assertNever(need);
  }
}

/** The source's own translated name, resolved by `t()` at render time. */
function sourceLabel(entry: SourceNeedEntryDto): MessageDescriptor {
  return descriptor(DATABASE_LABEL_KEYS[entry.database]);
}

/**
 * One line of the needs-action list: what a click does for this source. A source whose
 * last download failed says so, with the failure's own headline (`locale` formats a rate
 * limit's retry instant).
 */
export function describeSourceNeed(entry: SourceNeedEntryDto, locale: string): MessageDescriptor {
  const source = sourceLabel(entry);
  const need = entry.need;
  switch (need.kind) {
    case "turnOn":
      return descriptor("dashboard.guide.getRules.sourceTurnOn", { source });
    case "download":
      return need.lastFailure === null
        ? descriptor("dashboard.guide.getRules.sourceDownload", { source })
        : descriptor("dashboard.guide.getRules.sourceRetry", {
            source,
            reason: describeFetchFailure(need.lastFailure, locale),
          });
    case "import":
      return descriptor("dashboard.guide.getRules.sourceImport", { source });
    default:
      return assertNever(need);
  }
}

/** Whether the Steam Workshop database has to be downloaded (the 49 MB source). */
function downloadsSteam(sources: readonly SourceNeedEntryDto[]): boolean {
  return sources.some((entry) => entry.database === "steam" && needsNetwork(entry.need));
}

/**
 * The offered click's label for what it would do. The 49 MB size appears only when the Steam
 * Workshop database has to be downloaded; "Try again" replaces "Download and import" once any
 * download needing one failed last time; "Import" when nothing goes over the network.
 */
export function recommendedRulesButtonLabel(
  sources: readonly SourceNeedEntryDto[],
): MessageDescriptor {
  if (!sources.some((entry) => needsNetwork(entry.need))) {
    return descriptor("dashboard.guide.getRules.importButton");
  }
  const isRetry = sources.some(
    (entry) => entry.need.kind === "download" && entry.need.lastFailure !== null,
  );
  if (isRetry) {
    return descriptor(
      downloadsSteam(sources)
        ? "dashboard.guide.getRules.retryButtonWithSteam"
        : "dashboard.guide.getRules.retryButton",
    );
  }
  return descriptor(
    downloadsSteam(sources)
      ? "dashboard.guide.getRules.downloadButtonWithSteam"
      : "dashboard.guide.getRules.downloadButton",
  );
}

/**
 * The skipped row's "Get them now" label: the same disclosure as the offered row, so the size
 * shows whenever Steam would be downloaded (and turned on) by the click.
 */
export function skippedButtonLabel(sources: readonly SourceNeedEntryDto[]): MessageDescriptor {
  return descriptor(
    downloadsSteam(sources)
      ? "dashboard.guide.getRules.getNowButtonWithSteam"
      : "dashboard.guide.getRules.getNowButton",
  );
}

/**
 * The label of the click a step offers, or `null` when it offers none. Captured when the click
 * starts so the running button keeps the wording the user pressed.
 */
export function rulesClickLabel(step: RecommendedRulesStepDto): MessageDescriptor | null {
  switch (step.kind) {
    case "needsAction":
      return recommendedRulesButtonLabel(step.sources);
    case "skipped":
      return skippedButtonLabel(step.sources);
    case "done":
    case "unavailable":
    case "inProgress":
      return null;
    default:
      return assertNever(step);
  }
}

/**
 * Why step 1 is unavailable: the sentence, and whether it links to Settings (only the
 * internet-access switch can be changed there; the first-run notice is answered above).
 */
export function describeUnavailable(reason: UnavailableReasonDto): {
  readonly message: MessageDescriptor;
  readonly linksToSettings: boolean;
} {
  switch (reason) {
    case "networkOff":
      return { message: descriptor("dashboard.guide.getRules.networkOff"), linksToSettings: true };
    case "awaitingFirstRun":
      return {
        message: descriptor("dashboard.guide.getRules.awaitingFirstRun"),
        linksToSettings: false,
      };
    default:
      return assertNever(reason);
  }
}

/** The status line for the latest progress tick of a running click. */
export function describeRecommendedRulesProgress(
  progress: RecommendedRulesProgressEventDto,
): MessageDescriptor {
  switch (progress.kind) {
    case "downloading":
      return descriptor("dashboard.guide.getRules.downloading", {
        source: descriptor(DATABASE_LABEL_KEYS[progress.database]),
      });
    case "importing":
      return descriptor("dashboard.guide.getRules.importing");
    default:
      return assertNever(progress);
  }
}

/** Whether the slow-connection note applies: the 49 MB Steam Workshop database is downloading. */
export function isSteamDownload(progress: RecommendedRulesProgressEventDto | null): boolean {
  return progress !== null && progress.kind === "downloading" && progress.database === "steam";
}

/** Whether any source the click tried to download did not come back updated or unchanged. */
function hasUnfetchedDownload(report: RecommendedRulesReportDto): boolean {
  return report.downloads.some(
    (result) => result.outcome.kind !== "updated" && result.outcome.kind !== "unchanged",
  );
}

/** The sentence for each way an import can fail; the importer's own text is not in it. */
function describeImportFailure(code: ImportFailureCodeDto): MessageDescriptor {
  switch (code) {
    case "importerFailed":
      return descriptor("dashboard.guide.getRules.resultImportFailedReadingFile");
    case "savingRulesFailed":
      return descriptor("dashboard.guide.getRules.resultImportFailedSaving");
    default:
      return assertNever(code);
  }
}

/**
 * The result toast's sentence for a finished click. Outcomes: all good, partial (some
 * downloads failed, what downloaded was imported), nothing downloaded, nothing to do (an
 * already-done rerun), import failed (one sentence per failure code), import record not
 * saved, profile changed mid-run.
 */
export function describeRecommendedRulesReport(
  report: RecommendedRulesReportDto,
): MessageDescriptor {
  const step = report.import;
  switch (step.kind) {
    case "imported":
      return hasUnfetchedDownload(report)
        ? descriptor("dashboard.guide.getRules.resultPartial")
        : descriptor("dashboard.guide.getRules.resultDone");
    case "notNeeded":
      if (hasUnfetchedDownload(report)) {
        return descriptor("dashboard.guide.getRules.resultDownloadFailed");
      }
      return report.downloads.length === 0
        ? descriptor("dashboard.guide.getRules.resultNothingToDo")
        : descriptor("dashboard.guide.getRules.resultDone");
    case "importedManifestNotRecorded":
      return descriptor("dashboard.guide.getRules.resultManifestNotRecorded");
    case "failed":
      return describeImportFailure(step.code);
    case "profileChanged":
      return descriptor("dashboard.guide.getRules.resultProfileChanged");
    default:
      return assertNever(step);
  }
}

/**
 * The backend's own English text for a failed import, shown only as the toast's
 * technical-details line (the same role as `CommandErrorDescriptor.technicalDetail`); `null`
 * for every other outcome.
 */
export function recommendedRulesReportTechnicalDetail(
  report: RecommendedRulesReportDto,
): string | null {
  return report.import.kind === "failed" ? report.import.message : null;
}

/** How the result toast is styled: PrimeVue's `"warn"`, never `"warning"`. */
export type ReportSeverity = "success" | "info" | "warn" | "error";

/** The toast severity matching {@link describeRecommendedRulesReport}'s sentence. */
export function recommendedRulesReportSeverity(report: RecommendedRulesReportDto): ReportSeverity {
  const step = report.import;
  switch (step.kind) {
    case "imported":
      return hasUnfetchedDownload(report) ? "warn" : "success";
    case "notNeeded":
      if (hasUnfetchedDownload(report)) {
        return "error";
      }
      return report.downloads.length === 0 ? "info" : "success";
    case "importedManifestNotRecorded":
    case "profileChanged":
      return "warn";
    case "failed":
      return "error";
    default:
      return assertNever(step);
  }
}
