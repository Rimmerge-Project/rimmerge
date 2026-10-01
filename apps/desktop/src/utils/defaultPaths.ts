import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { DefaultPathsErrorDto } from "@/types/generated/DefaultPathsErrorDto";
import type { DefaultPathsWarningDto } from "@/types/generated/DefaultPathsWarningDto";
import { assertNever } from "@/utils/assertNever";

/**
 * Localizes `get_default_paths`'s structured, `kind`-tagged
 * `warningCode` — the Setup page's own resolved-but-suspicious-install
 * notice. The plain-string `warning` field stays as the "technical
 * details" fallback for one release (see `apps/desktop/CLAUDE.md`'s
 * i18n conventions).
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
