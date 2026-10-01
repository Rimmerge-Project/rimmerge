import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { CheckForUpdateOutcomeDto } from "@/types/generated/CheckForUpdateOutcomeDto";
import type { NotificationActionDto } from "@/types/generated/NotificationActionDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import type { NotificationKindDto } from "@/types/generated/NotificationKindDto";
import { assertNever } from "@/utils/assertNever";
import { describeFetchFailure, fetchFailureTechnicalDetail } from "@/utils/fetchFailure";

/** One `NotificationKindDto`'s own display name — Settings → Network's muted-kinds list. */
export function notificationKindLabel(kind: NotificationKindDto): MessageDescriptor {
  switch (kind) {
    case "welcome":
      return descriptor("notifications.welcome.title");
    case "gameVersionChanged":
      return descriptor("notifications.gameVersionChanged.kindLabel");
    case "updateAvailable":
      return descriptor("notifications.updateAvailable.kindLabel");
    case "ruleDatabasesStale":
      return descriptor("notifications.ruleDatabasesStale.kindLabel");
    case "importedRulesOutdated":
      return descriptor("notifications.importedRulesOutdated.kindLabel");
    case "recommendedSourcesIncomplete":
      return descriptor("notifications.recommendedSources.kindLabel");
    default:
      return assertNever(kind);
  }
}

/**
 * One notice's title, as a {@link MessageDescriptor} — an exhaustive
 * switch over `data.kind`, ending in `assertNever` so a new
 * `NotificationDataDto` variant is a compile error here, not a silent
 * gap. Rust never phrases this text (see `apps/desktop/CLAUDE.md`); it
 * sends only the codes/values these descriptors interpolate.
 */
export function notificationTitle(notification: NotificationDto): MessageDescriptor {
  const data = notification.data;
  switch (data.kind) {
    case "welcome":
      return descriptor("notifications.welcome.title");
    case "gameVersionChanged":
      return descriptor("notifications.gameVersionChanged.title");
    case "updateAvailable":
      return descriptor("notifications.updateAvailable.title", {
        version: data.latestVersion,
      });
    case "ruleDatabasesStale": {
      const count = Object.keys(data.sources).length;
      return descriptor("notifications.ruleDatabasesStale.title", { count }, count);
    }
    case "importedRulesOutdated": {
      const count = Object.keys(data.sources).length;
      return descriptor("notifications.importedRulesOutdated.title", { count }, count);
    }
    case "recommendedSourcesIncomplete": {
      const count = Object.keys(data.sources).length;
      return descriptor("notifications.recommendedSources.title", { count }, count);
    }
    default:
      return assertNever(data);
  }
}

/** One notice's body — see {@link notificationTitle}'s own doc comment. */
export function notificationBody(notification: NotificationDto): MessageDescriptor {
  const data = notification.data;
  switch (data.kind) {
    case "welcome":
      return descriptor("notifications.welcome.body");
    case "gameVersionChanged":
      return descriptor("notifications.gameVersionChanged.body", {
        acknowledged: data.acknowledged,
        current: data.current,
      });
    case "updateAvailable":
      return descriptor("notifications.updateAvailable.body", {
        version: data.latestVersion,
        running: data.running,
      });
    case "ruleDatabasesStale": {
      const count = Object.keys(data.sources).length;
      return descriptor("notifications.ruleDatabasesStale.body", { count }, count);
    }
    case "importedRulesOutdated": {
      const count = Object.keys(data.sources).length;
      return descriptor("notifications.importedRulesOutdated.body", { count }, count);
    }
    case "recommendedSourcesIncomplete":
      // A structurally different sentence when the large download is in
      // the set, so it gets its own key rather than a spliced fragment.
      return data.sources.steam === undefined
        ? descriptor("notifications.recommendedSources.body")
        : descriptor("notifications.recommendedSources.bodyWithSteam");
    default:
      return assertNever(data);
  }
}

/**
 * "Check now"'s own result line — see {@link notificationTitle}'s own
 * doc comment for the exhaustiveness convention this follows. `locale`
 * formats the rate-limit reason's own instant through the app locale,
 * never the host locale a bare `new Date(...).toString()` would use.
 */
export function describeCheckForUpdateOutcome(
  outcome: CheckForUpdateOutcomeDto,
  locale: string,
): MessageDescriptor {
  if (outcome.kind === "skipped") {
    const reason = outcome.reason;
    if (typeof reason === "object") {
      return descriptor("settings.network.checkNow.rateLimited", {
        until: new Date(reason.rateLimitedUntil.until).toLocaleString(locale),
      });
    }
    switch (reason) {
      case "awaitingFirstRun":
        return descriptor("settings.network.checkNow.awaitingFirstRun");
      case "alreadyRanThisLaunch":
        return descriptor("settings.network.checkNow.alreadyRanThisLaunch");
      case "networkDisabled":
        return descriptor("settings.network.checkNow.networkDisabled");
      case "checkDisabled":
        return descriptor("settings.network.checkNow.checkDisabled");
      case "notDue":
        return descriptor("settings.network.checkNow.notDue");
      default:
        return assertNever(reason);
    }
  }

  const runOutcome = outcome.outcome;
  switch (runOutcome.kind) {
    case "updated":
      return descriptor("settings.network.checkNow.updated", {
        version: runOutcome.latestVersion,
      });
    case "unchanged":
      return descriptor("settings.network.checkNow.unchanged");
    case "failed":
      return descriptor("settings.network.checkNow.failed", {
        reason: describeFetchFailure(runOutcome.failure, locale),
      });
    default:
      return assertNever(runOutcome);
  }
}

/**
 * The technical-details line of a failed "Check now", `null` for any other
 * outcome (and for a failure whose headline already says it all). Rendered
 * as its own line beside {@link describeCheckForUpdateOutcome}'s headline.
 */
export function checkForUpdateTechnicalDetail(outcome: CheckForUpdateOutcomeDto): string | null {
  if (outcome.kind === "skipped") {
    return null;
  }
  const runOutcome = outcome.outcome;
  return runOutcome.kind === "failed" ? fetchFailureTechnicalDetail(runOutcome.failure) : null;
}

/** One action button's label — see {@link notificationTitle}'s own doc comment. */
export function notificationActionLabel(action: NotificationActionDto): MessageDescriptor {
  switch (action) {
    case "keepNetworkSettings":
      return descriptor("notifications.actions.keepNetworkSettings");
    case "turnOffNetwork":
      return descriptor("notifications.actions.turnOffNetwork");
    case "applyRecommendedSettings":
      return descriptor("notifications.actions.applyRecommendedSettings");
    case "refreshRuleDatabases":
      return descriptor("notifications.actions.refreshRuleDatabases");
    case "openRuleDatabases":
      return descriptor("notifications.actions.openRuleDatabases");
    case "openSettings":
      return descriptor("notifications.actions.openSettings");
    case "showReleasePage":
      return descriptor("notifications.actions.showReleasePage");
    case "openPatches":
      return descriptor("notifications.actions.openPatches");
    case "enableRecommendedSources":
      return descriptor("notifications.actions.enableRecommendedSources");
    default:
      return assertNever(action);
  }
}
