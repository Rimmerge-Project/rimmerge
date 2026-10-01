// The verify pass.

import type { VerifyReportDto } from "@/types/generated/VerifyReportDto";
import type { VerifyRequestDto } from "@/types/generated/VerifyRequestDto";
import { call } from "./core";

/**
 * Runs `rim_session::use_cases::VerifyOrder`
 * against the currently loaded session — replays every foreign patch
 * operation to predict which would fail under `request.source`'s order,
 * the same replay `defs inspect`/the merge preview already trust.
 * Explicit, on-demand only (a long pass on a large install — see
 * `VerifyProgressEventDto`'s own doc comment) — never called from
 * anywhere but the apply dialog's own "Verify order" button.
 */
export function verifyOrder(request: VerifyRequestDto): Promise<VerifyReportDto> {
  return call("verify_order", { request });
}
