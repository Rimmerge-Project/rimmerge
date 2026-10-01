// Notifications: the active-notice list, dismiss/mute, the first-run notice, and the update check.

import type { CheckForUpdateOutcomeDto } from "@/types/generated/CheckForUpdateOutcomeDto";
import type { LaunchNetworkChecksOutcomeDto } from "@/types/generated/LaunchNetworkChecksOutcomeDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import type { NotificationKeyDto } from "@/types/generated/NotificationKeyDto";
import type { NotificationKindDto } from "@/types/generated/NotificationKindDto";
import { call } from "./core";

/**
 * Every active notice, evaluated fresh against the current app settings,
 * this profile's rule-database status, and the running version. Requires
 * a loaded project.
 */
export function listNotifications(): Promise<NotificationDto[]> {
  return call("list_notifications");
}

/**
 * Dismisses one notice occurrence. For
 * `key.kind === "gameVersionChanged"`, the backend routes this to the
 * per-profile acknowledgement store instead of the app-global dismissed
 * list — the frontend calls the same command either way.
 */
export function dismissNotification(key: NotificationKeyDto): Promise<void> {
  return call("dismiss_notification", { key });
}

/** The set of muted notice kinds — Settings → Network's own "muted kinds" list. */
export function listMutedNotificationKinds(): Promise<NotificationKindDto[]> {
  return call("list_muted_notification_kinds");
}

/** Mutes a whole notice kind — "Don't remind me again". */
export function muteNotificationKind(kind: NotificationKindDto): Promise<void> {
  return call("mute_notification_kind", { kind });
}

/** Reverses {@link muteNotificationKind}. */
export function unmuteNotificationKind(kind: NotificationKindDto): Promise<void> {
  return call("unmute_notification_kind", { kind });
}

/** Answers the first-run (Welcome) notice. */
export function completeWelcome(): Promise<void> {
  return call("complete_welcome");
}

/**
 * Resets this profile's sorter/ledger settings to their built-in
 * defaults — the Welcome notice's own "Use recommended settings" action.
 * Saves immediately, unlike the Settings page's own "Reset to defaults"
 * (which only refills the form).
 */
export function resetSettings(): Promise<void> {
  return call("reset_settings");
}

/**
 * "Check now" (Settings → Network) — ignores the `checkForUpdates`
 * toggle and the 24h throttle (the click is its own consent), but still
 * honours `allowNetwork` and an active rate limit.
 */
export function checkForUpdate(): Promise<CheckForUpdateOutcomeDto> {
  return call("check_for_update");
}

/**
 * The once-per-launch automatic check — call once, fire-and-forget,
 * right after `load_project` succeeds. A second call in the same
 * process is safe (the backend self-gates) but does nothing; the caller
 * re-queries {@link listNotifications} afterward to see any result.
 */
export function runLaunchNetworkChecks(): Promise<LaunchNetworkChecksOutcomeDto> {
  return call("run_launch_network_checks");
}
