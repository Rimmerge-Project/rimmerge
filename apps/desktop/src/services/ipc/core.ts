// The one `invoke` wrapper every command goes through, and the error it rethrows.

import { invoke } from "@tauri-apps/api/core";
import type { CommandError } from "@/types/generated/CommandError";
import type { CommandErrorCode } from "@/types/generated/CommandErrorCode";
import type { CommandErrorDetail } from "@/types/generated/CommandErrorDetail";

/**
 * A Tauri command failed. Wraps the `{ code, message, detail }` the Rust
 * side returned so callers can `instanceof RimmergeError` and branch on
 * `.code` instead of parsing `.message` — `.detail` carries the
 * structured payload a handful of codes add (currently just
 * `assignment_section_in_use`'s `referencedBy` list) for a caller that
 * needs to render it as data rather than text.
 */
export class RimmergeError extends Error {
  readonly code: CommandErrorCode;
  readonly detail: CommandErrorDetail | null;

  constructor(commandError: CommandError) {
    super(commandError.message);
    this.name = "RimmergeError";
    this.code = commandError.code;
    this.detail = commandError.detail ?? null;
  }
}

/** Narrows an unknown rejection into the `CommandError` shape it should be. */
function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as { code: unknown }).code === "string" &&
    typeof (value as { message: unknown }).message === "string"
  );
}

/** The two codes that mean "this session is gone, there's nothing left to show" — see {@link setSessionLostHandler}. */
const SESSION_LOST_CODES: readonly CommandErrorCode[] = ["session_lost", "no_project_loaded"];

let sessionLostHandler: ((code: CommandErrorCode) => void) | null = null;

/**
 * Registers the single handler for `session_lost` (the backend's session
 * lock was poisoned by an earlier panic and discarded) and
 * `no_project_loaded` (no project was ever loaded, or a reset already
 * cleared it) — both mean the loaded project is simply gone, so every
 * page built on it should stop pretending otherwise and send the user
 * back to setup. Called once, from `main.ts`, which owns the session
 * store and the router; this module takes a plain callback instead of
 * importing either directly, so every IPC wrapper here keeps working
 * (module-load order, Vitest unit tests that never call this) whether or
 * not a Pinia/Vue Router instance exists yet.
 */
export function setSessionLostHandler(handler: (code: CommandErrorCode) => void): void {
  sessionLostHandler = handler;
}

/**
 * Invokes a Tauri command, rethrowing a failed invocation as a
 * {@link RimmergeError}. This is the only function in this module that
 * calls `invoke` directly — every command wrapper below goes through it.
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    if (isCommandError(error)) {
      if (SESSION_LOST_CODES.includes(error.code)) {
        sessionLostHandler?.(error.code);
      }
      throw new RimmergeError(error);
    }
    throw error;
  }
}
