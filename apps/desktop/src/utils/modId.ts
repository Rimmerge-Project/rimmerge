const STEAM_SUFFIX = "_steam";

/**
 * Strips RimWorld's `_steam` suffix (the Workshop copy's id) down to the
 * mod's base id. A no-op for a bare id. Mirrors Rust's `ModId::base()`:
 * every comparison between ids from different sources (a game log's
 * package ids, `ModsConfig.xml` order rows) goes through it.
 */
export function baseModId(id: string): string {
  return id.endsWith(STEAM_SUFFIX) ? id.slice(0, -STEAM_SUFFIX.length) : id;
}
