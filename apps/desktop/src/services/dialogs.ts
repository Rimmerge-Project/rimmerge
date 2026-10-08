import { open, save } from "@tauri-apps/plugin-dialog";

/**
 * Opens a native folder picker, seeded at `defaultPath` if given. Returns
 * `null` when the user cancels.
 */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    ...(defaultPath !== undefined && { defaultPath }),
  });
  return selected ?? null;
}

/** The already-translated labels of {@link pickFile}'s two filters. */
export type PickFileFilterNames = {
  readonly logFiles: string;
  readonly allFiles: string;
};

/**
 * Opens a native file picker for a game log — the import button on the
 * `/startup` page and the dashboard. It lists `.log` and `.txt` files first
 * (a `Player.log`, or an in-game console snapshot, which players save under
 * any name) and offers every file too, since the import detects a log's kind
 * from its content and never from its name. Returns `null` when the user
 * cancels. The filter names are the caller's translated labels, so the
 * picker's own filter list follows the app's language.
 * `dialog:allow-open` is already granted (`src-tauri/src/capabilities/default.json`);
 * no extra capability is needed for this file-mode use.
 */
export async function pickFile(filterNames: PickFileFilterNames): Promise<string | null> {
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [
      { name: filterNames.logFiles, extensions: ["log", "txt"] },
      { name: filterNames.allFiles, extensions: ["*"] },
    ],
  });
  return selected ?? null;
}

/** The already-translated labels of {@link pickModListFile}'s two filters. */
export type PickModListFilterNames = {
  readonly modLists: string;
  readonly allFiles: string;
};

/**
 * Opens a native file picker for a shared mod list: RimWorld's own `.rml`, a
 * ModsConfig.xml-shaped `.xml` or a `.txt` text list first, then every file, since the
 * import detects the format from content and never from the name. Returns `null` when the
 * user cancels. Uses `dialog:allow-open`, already granted.
 */
export async function pickModListFile(names: PickModListFilterNames): Promise<string | null> {
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [
      { name: names.modLists, extensions: ["rml", "xml", "txt"] },
      { name: names.allFiles, extensions: ["*"] },
    ],
  });
  return selected ?? null;
}

/** What {@link pickModListSavePath} needs from the caller. */
export type ModListSaveOptions = {
  /** The translated name of the `.rml` filter. */
  readonly filterName: string;
  /** Where the dialog starts (RimWorld's `ModLists` folder), or `null` for the OS default. */
  readonly defaultPath: string | null;
};

/**
 * Opens the native save dialog for a RimWorld mod list, offering only `.rml` (the one
 * extension the backend writes). Returns `null` when the user cancels. Needs
 * `dialog:allow-save`, which `dialog:allow-open` does not cover.
 */
export async function pickModListSavePath(options: ModListSaveOptions): Promise<string | null> {
  const selected = await save({
    filters: [{ name: options.filterName, extensions: ["rml"] }],
    ...(options.defaultPath !== null && { defaultPath: options.defaultPath }),
  });
  return selected ?? null;
}
