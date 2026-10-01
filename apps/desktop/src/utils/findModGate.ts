import { formatList } from "@/i18n/format";
import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { FindModGateDto } from "@/types/generated/FindModGateDto";
import { assertNever } from "@/utils/assertNever";

/**
 * One phrase per `PatchOperationFindMod` gate enclosing a patch operation:
 * "any of A, B, or C" (the gate opens when one of them is active) or
 * "none of A or B" (it opens only while none is). The gates themselves
 * stack, so the caller joins the phrases as a conjunction. `locale`
 * formats each mod list (`Intl.ListFormat`), never a hard-coded separator.
 */
export function describeFindModGate(gate: FindModGateDto, locale: string): MessageDescriptor {
  const mods = formatList(locale, gate.mods, "disjunction");
  switch (gate.kind) {
    case "anyActive":
      return descriptor("defPage.gate.anyActive", { mods });
    case "noneActive":
      return descriptor("defPage.gate.noneActive", { mods });
    default:
      return assertNever(gate);
  }
}
