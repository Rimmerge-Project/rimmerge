import { useMutation, useQuery } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import {
  getMod,
  getModInfo,
  listModNames,
  listMods,
  openModLink,
  readModIcon,
  readModPreview,
} from "@/services/ipc";
import type { ModFilterDto } from "@/types/generated/ModFilterDto";
import type { ModLinkKindDto } from "@/types/generated/ModLinkKindDto";

/**
 * Searches and pages the active mod list. `enabled` (default: always)
 * lets a caller with more than one tab of mod lists — `ModsPage.vue`'s
 * own Active/Inactive split — skip the query entirely
 * while its own tab isn't showing, rather than fetching both on every
 * keystroke.
 */
export function useModsQuery(
  filter: MaybeRefOrGetter<ModFilterDto>,
  enabled: MaybeRefOrGetter<boolean> = true,
) {
  return useQuery({
    key: () => queryKeys.mods(toValue(filter)),
    query: () => listMods(toValue(filter)),
    enabled: () => toValue(enabled),
  });
}

/** One mod's full detail. Disabled while `modId` is `null`. */
export function useModQuery(modId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.mod(toValue(modId) ?? ""),
    query: () => getMod(toValue(modId) ?? ""),
    enabled: () => toValue(modId) !== null,
  });
}

/**
 * Every active mod's id mapped to its display name — the source
 * `useModLabel` reads from. Invalidated with everything else, so
 * a decision or import that changes which mods are active refreshes it
 * along with every other query.
 */
export function useModNamesQuery() {
  return useQuery({
    key: () => queryKeys.modNames(),
    query: () => listModNames(),
  });
}

/**
 * One mod's full information for the mod info panel — active, inactive,
 * or missing, plus its lazily-read `About.xml` details. Disabled while
 * `modId` is `null` (no selection).
 */
export function useModInfoQuery(modId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.modInfo(toValue(modId) ?? ""),
    query: () => getModInfo(toValue(modId) as string),
    enabled: () => toValue(modId) !== null,
  });
}

/**
 * `modId`'s own `About/Preview.png`, for the mod info panel's image.
 * Disabled while `modId` is `null`.
 *
 * `staleTime: Infinity` — a preview's bytes never change within a loaded
 * project, the same reasoning `useTextureQuery` documents. `gcTime:
 * 30_000` (unlike `useTextureQuery`, which never overrides it): at
 * roughly 330 KB of base64 per p50 preview, the default 5-minute garbage
 * collection would keep hundreds of MB of previews alive after someone
 * arrows through a few hundred rows — thirty seconds keeps quick
 * back-and-forth instant while bounding memory to whatever was recently
 * viewed.
 */
export function useModPreviewQuery(modId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.modPreview(toValue(modId) ?? ""),
    query: () => readModPreview(toValue(modId) as string),
    enabled: () => toValue(modId) !== null,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30_000,
  });
}

/**
 * `modId`'s own `About/ModIcon.png`, for the mod info panel's small
 * icon. Same caching shape as {@link useModPreviewQuery}.
 */
export function useModIconQuery(modId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.modIcon(toValue(modId) ?? ""),
    query: () => readModIcon(toValue(modId) as string),
    enabled: () => toValue(modId) !== null,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30_000,
  });
}

/**
 * Opens a mod's workshop or homepage link in the system browser — a
 * mutation (an explicit side effect, not cached data) even though
 * nothing in the session changes; `openModLink` never sends a URL, only
 * `modId` and which link.
 */
export function useOpenModLinkMutation() {
  return useMutation({
    mutation: ({ modId, link }: { modId: string; link: ModLinkKindDto }) =>
      openModLink(modId, link),
  });
}
