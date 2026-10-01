import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";

/** What an Apply click will write, as the dialog's two checkboxes say. */
export type ApplyWrites = {
  /** `ModsConfig.xml` (the load order RimWorld reads). */
  writeModsConfig: boolean;
  /** The generated merge mod under `Mods/`. */
  writeMergeMod: boolean;
};

/**
 * The Apply dialog's primary button label for the two write checkboxes.
 * Writing `ModsConfig.xml` is "Apply" whether or not the merge mod is
 * written too; without it the label names what is left. Both flags are
 * booleans, so the guards below cover every combination.
 */
export function applyButtonLabel(writes: ApplyWrites): MessageDescriptor {
  if (writes.writeModsConfig) {
    return descriptor("apply.dialog.applyButton");
  }
  if (writes.writeMergeMod) {
    return descriptor("apply.dialog.writeMergeModButton");
  }
  return descriptor("apply.dialog.saveDecisionsButton");
}
