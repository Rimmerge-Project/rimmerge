import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { DefaultPathsErrorDto } from "@/types/generated/DefaultPathsErrorDto";
import type { DefaultPathsWarningDto } from "@/types/generated/DefaultPathsWarningDto";
import { assertNever } from "@/utils/assertNever";

/**
 * Localizes `get_default_paths`'s structured, `kind`-tagged
 * `warningCode` — the Setup page's own resolved-but-suspicious-install
 * notice. The directory crosses as a path parameter; the sentence is
 * worded here, never sent by the backend.
 */
export function describeDefaultPathsWarning(warning: DefaultPathsWarningDto): MessageDescriptor {
  // Exactly one variant today, so ts-rs emits `DefaultPathsWarningDto`
  // as a single object type, not a union — an `assertNever`-terminated
  // `switch` needs a real discriminated union to narrow against, so
  // this returns directly instead. Revisit as a switch once a second
  // variant lands (see this type's own Rust doc comment).
  return descriptor("defaultPathsWarning.notAnInstall", { gameDir: warning.gameDir });
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
