import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import { useCompleteWelcomeMutation, useResetSettingsMutation } from "@/queries/notifications";
import { useRefreshRuleDatabasesMutation } from "@/queries/rules";
import {
  useAppSettingsQuery,
  useEnableRecommendedSourcesMutation,
  useUpdateAppSettingsMutation,
} from "@/queries/settings";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { NotificationActionDto } from "@/types/generated/NotificationActionDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import type { RuleDatabaseDto } from "@/types/generated/RuleDatabaseDto";
import type { RuleDatabasesStaleDto } from "@/types/generated/RuleDatabasesStaleDto";
import { assertNever } from "@/utils/assertNever";
import { toastInfo } from "@/utils/toast";

/** The `<owner>/<repo>/releases/tag/` prefix a stable, parsed version is appended to — never anything from the response body itself (see `docs/privacy-and-network.md`). */
const RELEASE_URL_PREFIX = "https://github.com/Rimmerge-Project/rimmerge/releases/tag/v";

/** The release page URL for one `UpdateAvailable` notice — built entirely on this side from a `const` prefix and the already-parsed version, never anything read out of a GitHub response body. */
export function releaseUrl(latestVersion: string): string {
  return `${RELEASE_URL_PREFIX}${latestVersion}`;
}

/**
 * The sources a stale-databases notice lists, sorted. The keys of a
 * `{ [key in RuleDatabaseDto]?: ... }` map are exactly `RuleDatabaseDto`
 * values, which `Object.keys` cannot say, hence the one cast.
 */
function staleSources(data: RuleDatabasesStaleDto): RuleDatabaseDto[] {
  return (Object.keys(data.sources) as RuleDatabaseDto[]).sort();
}

/**
 * Runs one {@link NotificationActionDto} — the shared handler behind
 * every action button, whether it's clicked from `NotificationItem.vue`
 * (the bell) or `WelcomeCard.vue` (the Dashboard), so the two surfaces
 * can never disagree about what a button does. Never dismisses the
 * notice itself: `completeWelcome` naturally removes `Welcome` (its own
 * condition is `welcome_completed_at`), everything else stays until the
 * underlying fact changes or the user dismisses it explicitly.
 */
export function useNotificationActions() {
  const { t } = useI18n();
  const router = useRouter();
  const { data: appSettings, refresh: refreshAppSettings } = useAppSettingsQuery();
  const { mutateAsync: saveAppSettings } = useUpdateAppSettingsMutation();
  const { mutateAsync: completeWelcome } = useCompleteWelcomeMutation();
  const { mutateAsync: applyRecommendedSettings } = useResetSettingsMutation();
  const { mutateAsync: refreshRuleDatabases } = useRefreshRuleDatabasesMutation();
  const { mutateAsync: enableRecommendedSources } = useEnableRecommendedSourcesMutation();

  /**
   * The loaded app settings, fetching them first if the query hasn't
   * resolved yet — never silently skips the caller's save the way a bare
   * `if (appSettings.value)` guard would. Throws (via `refresh`'s own
   * `throwOnError`) rather than returning a placeholder when settings
   * are genuinely unavailable: a caller here is always about to persist
   * a privacy-relevant change, and answering Welcome without that change
   * having actually saved would silently leave the network gate open.
   */
  async function loadedAppSettings(): Promise<AppSettingsDto> {
    if (appSettings.value) {
      return appSettings.value.settings;
    }
    const state = await refreshAppSettings(true);
    if (!state.data) {
      throw new Error("cannot proceed: app settings failed to load");
    }
    return state.data.settings;
  }

  async function runAction(
    action: NotificationActionDto,
    notification: NotificationDto,
  ): Promise<void> {
    switch (action) {
      case "keepNetworkSettings":
        await completeWelcome();
        return;
      case "turnOffNetwork": {
        const settings = await loadedAppSettings();
        await saveAppSettings({
          ...settings,
          network: { ...settings.network, allowNetwork: false },
        });
        await completeWelcome();
        toastInfo(t("notifications.actions.turnOffNetworkToast"));
        return;
      }
      case "applyRecommendedSettings":
        await applyRecommendedSettings();
        await completeWelcome();
        toastInfo(t("notifications.actions.recommendedSettingsAppliedToast"));
        return;
      case "refreshRuleDatabases": {
        // Refreshes exactly the sources this notice lists, never the other
        // enabled ones: the Steam Workshop database is about 49 MB and a
        // notice that does not name it must not download it.
        if (notification.data.kind !== "ruleDatabasesStale") {
          return;
        }
        await refreshRuleDatabases({ sources: staleSources(notification.data) });
        return;
      }
      case "enableRecommendedSources": {
        // The click is the consent for both steps: Rust turns the
        // recommended sources on, then the ordinary manual refresh
        // downloads them. Importing stays a separate, manual step.
        await enableRecommendedSources();
        const outcomes = await refreshRuleDatabases(null);
        const hasFailure = outcomes.some(({ outcome }) => outcome.kind === "failed");
        toastInfo(
          t(
            hasFailure
              ? "notifications.actions.enableRecommendedSourcesPartialToast"
              : "notifications.actions.enableRecommendedSourcesToast",
          ),
        );
        return;
      }
      case "openRuleDatabases":
        await router.push({ name: "rules" });
        return;
      case "openSettings":
        await router.push({ name: "settings" });
        return;
      case "showReleasePage": {
        if (notification.data.kind !== "updateAvailable") {
          return;
        }
        try {
          await navigator.clipboard.writeText(releaseUrl(notification.data.latestVersion));
          toastInfo(t("notifications.actions.linkCopiedToast"));
        } catch {
          // Clipboard access can be blocked (permissions, a non-secure
          // context in some webviews); `NotificationItem.vue` also
          // renders this same URL as selectable plain text below the
          // notice body (its own `releaseLink`), so this is a
          // convenience failure with a visible fallback already in
          // place, not a broken invariant worth an error toast.
        }
        return;
      }
      case "openPatches":
        await router.push({ name: "patches" });
        return;
      default:
        assertNever(action);
    }
  }

  return { runAction };
}
