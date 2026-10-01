import { open } from "@tauri-apps/plugin-dialog";

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
