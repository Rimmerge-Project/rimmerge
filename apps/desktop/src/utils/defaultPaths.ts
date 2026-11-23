import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { DefaultPathsErrorDto } from "@/types/generated/DefaultPathsErrorDto";
import type { DefaultPathsWarningDto } from "@/types/generated/DefaultPathsWarningDto";
import { assertNever } from "@/utils/assertNever";

/**
 * Localizes `get_default_paths`'s structured, `kind`-tagged `warningCode`: a
 * resolved-but-suspicious install, or a path the backend refused to use. Paths
 * cross as parameters and every sentence is worded here, never sent by the backend.
 */
export function describeDefaultPathsWarning(warning: DefaultPathsWarningDto): MessageDescriptor {
  switch (warning.kind) {
    case "notAnInstall":
      return descriptor("defaultPathsWarning.notAnInstall", { gameDir: warning.gameDir });
    case "pathRefused":
      return describePathRefused(warning);
    default:
      return assertNever(warning);
  }
}

type PathRefused = Extract<DefaultPathsWarningDto, { kind: "pathRefused" }>;

/**
 * A typed text is only ever the game folder, so its two sentences name no field and carry
 * the advice a user can act on (map the share to a drive letter). A stored value (an
 * environment variable or `config.json`) is not on screen, so its sentence only says it was
 * ignored and names the field.
 */
function describePathRefused(warning: PathRefused): MessageDescriptor {
  switch (warning.source) {
    case "typed":
      return descriptor(
        warning.reason === "networkShare"
          ? "defaultPathsWarning.typedNetworkShare"
          : "defaultPathsWarning.typedInvalid",
      );
    case "configured":
      return descriptor(configuredKey(warning.field));
    default:
      return assertNever(warning.source);
  }
}

function configuredKey(field: PathRefused["field"]): string {
  switch (field) {
    case "gameDir":
      return "defaultPathsWarning.configuredGameDir";
    case "workshopDir":
      return "defaultPathsWarning.configuredWorkshopDir";
    case "modsConfig":
      return "defaultPathsWarning.configuredModsConfig";
    default:
      return assertNever(field);
  }
}

/**
 * Whether the warning answers for the game folder field's own text (so it is hidden while
 * the user edits the field), as opposed to a stored value that stays true whatever is typed.
 */
export function warningConcernsTheField(warning: DefaultPathsWarningDto): boolean {
  switch (warning.kind) {
    case "notAnInstall":
      return true;
    case "pathRefused":
      return warning.source === "typed";
    default:
      return assertNever(warning);
  }
}

/** Localizes `get_default_paths`'s structured, `kind`-tagged `errorCode` — see {@link describeDefaultPathsWarning}. */
export function describeDefaultPathsError(error: DefaultPathsErrorDto): MessageDescriptor {
  switch (error.kind) {
    case "noProfileBase":
      return descriptor("defaultPathsError.noProfileBase");
    case "gameDirNotFound":
      return descriptor("defaultPathsError.gameDirNotFound");
    case "modsConfigNotFound":
      return descriptor("defaultPathsError.modsConfigNotFound");
    default:
      return assertNever(error);
  }
}

/**
 * The locations `get_default_paths` checked for an install, in the order
 * it tried them: non-empty only for `gameDirNotFound`. File paths, shown
 * untranslated in `<code>` elements.
 */
export function checkedLocations(error: DefaultPathsErrorDto): readonly string[] {
  switch (error.kind) {
    case "gameDirNotFound":
      return error.candidates;
    case "noProfileBase":
    case "modsConfigNotFound":
      return [];
    default:
      return assertNever(error);
  }
}
