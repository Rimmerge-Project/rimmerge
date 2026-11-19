// The live ModsConfig.xml baseline the Mods page marks pending changes against.

import type { FileBaselineDto } from "@/types/generated/FileBaselineDto";
import { call } from "./core";

/** The baseline's status and what someone else changed in the file. Read-only. */
export function getModsConfigBaseline(): Promise<FileBaselineDto> {
  return call("get_mods_config_baseline");
}

/**
 * Re-reads the file and swaps the result into the session. Only reads; the backend emits
 * `session://changed` (reason `fileBaselineChanged`) only when the list or the status changed.
 */
export function refreshModsConfigBaseline(): Promise<FileBaselineDto> {
  return call("refresh_mods_config_baseline");
}
