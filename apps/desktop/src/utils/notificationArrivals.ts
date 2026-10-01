import type { NotificationDto } from "@/types/generated/NotificationDto";

/** A notice occurrence's own stable identity — shared by every place that diffs one notice list against another. */
export function notificationKeyId(notification: NotificationDto): string {
  return `${notification.key.kind}:${notification.key.fingerprint}`;
}

/**
 * The notices in `current` absent from `previousKeys` — a genuine new
 * arrival, never a plain re-render of an unchanged list. `previousKeys`
 * `null` means "no list has been seen yet" (the very first load), which
 * always returns no arrivals: the live region this feeds
 * (`TheShell.vue`) must never announce on mount, only on a real change
 * afterward. A decrease (a dismiss/mute) also returns no arrivals, since
 * every one of `current`'s own notices was already in `previousKeys`.
 */
export function newArrivals(
  previousKeys: ReadonlySet<string> | null,
  current: readonly NotificationDto[],
): readonly NotificationDto[] {
  if (!previousKeys) {
    return [];
  }
  return current.filter((notification) => !previousKeys.has(notificationKeyId(notification)));
}
