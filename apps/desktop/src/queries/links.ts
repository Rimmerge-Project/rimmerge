import { useMutation, useQuery } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import { getAppVersion, openAppLink } from "@/services/ipc";
import type { AppLinkTargetDto } from "@/types/generated/AppLinkTargetDto";

/**
 * This build's own version string, for the Settings page's About
 * section. Never changes while the app is running, so the default
 * `staleTime` costs nothing extra — kept as a query rather than a
 * one-shot call so every mount shares the same cached fetch.
 */
export function useAppVersionQuery() {
  return useQuery({
    key: () => queryKeys.appVersion(),
    query: () => getAppVersion(),
  });
}

/**
 * Opens one of the app's own fixed external links (the GitHub repo, its
 * issues page, the support page) in the system browser — a mutation, the
 * same shape `useOpenModLinkMutation` uses. `openAppLink` never sends a
 * URL, only which target.
 */
export function useOpenAppLinkMutation() {
  return useMutation({
    mutation: (target: AppLinkTargetDto) => openAppLink(target),
  });
}
