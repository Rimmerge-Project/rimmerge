import type { PatchFailureSummaryDto } from "@/types/generated/PatchFailureSummaryDto";
import type { VerifyOperationDto } from "@/types/generated/VerifyOperationDto";

/**
 * Whitespace-collapse, mirroring `rim_resolve::domain::normalize_log_text`
 * (`text.split_whitespace().collect::<Vec<_>>().join(" ")`) — the join
 * key both sides of the predicted-vs-observed comparison must go
 * through, since a captured operation text can carry literal newlines/
 * tabs from a multi-line xpath while the log renders one line.
 *
 * **The mirror is approximate, not exact**: JS's `\s` in a regex matches
 * the Unicode `White_Space` property, which includes U+FEFF (BOM) among a
 * few other codepoints Rust's `char::is_whitespace` (what
 * `split_whitespace` uses) does not. A captured operation text with a
 * stray BOM would collapse it away on this side and keep it on the Rust
 * side, joining differently there than here. Accepted: no shared fixture
 * exists between the two languages to assert against, and a real RimWorld
 * log/verify capture has never been observed to contain one.
 */
export function normalizeLogText(text: string): string {
  const trimmed = text.trim();
  return trimmed.length === 0 ? "" : trimmed.split(/\s+/).join(" ");
}

/** One `(mod, operation)` pair with both sides of the predicted-vs-observed join present. */
export interface MatchedOperation {
  modId: string;
  operation: string;
  predicted: VerifyOperationDto;
  /** Usually one entry — kept as a list since the log could in principle log the same operation's failure twice. */
  observed: PatchFailureSummaryDto[];
}

/** The predicted-vs-observed join result. */
export interface PredictedVsObserved {
  /** Predicted (by `VerifyOrder`) and confirmed by the imported log. */
  matched: MatchedOperation[];
  /** Predicted, but the imported log reports no matching failure. */
  predictedOnly: VerifyOperationDto[];
  /**
   * Observed in the log but not predicted — includes every unattributed
   * failure (a raw log line resolving to no active mod can never join
   * against a `VerifyOperationDto`, which is always mod-scoped) — never
   * dropped.
   */
  observedOnly: PatchFailureSummaryDto[];
}

/**
 * `modId -> normalizeLogText(operation) -> observed failures`, a nested
 * `Map` rather than one string-concatenated key. A concatenated key
 * (`` `${modId} ${operation}` ``) can
 * collide: `("a.mod", "x y")` and `("a.mod x", "y")` both produce the
 * literal string `"a.mod x y"`.
 * Nesting the maps makes that class of bug structurally impossible —
 * `modId` and `operation` are never concatenated into one comparable
 * value at all.
 */
type ByModThenOperation = Map<string, Map<string, PatchFailureSummaryDto[]>>;

function pushObserved(
  byMod: ByModThenOperation,
  modId: string,
  normalizedOperation: string,
  failure: PatchFailureSummaryDto,
): void {
  let byOperation = byMod.get(modId);
  if (!byOperation) {
    byOperation = new Map();
    byMod.set(modId, byOperation);
  }
  const bucket = byOperation.get(normalizedOperation);
  if (bucket) {
    bucket.push(failure);
  } else {
    byOperation.set(normalizedOperation, [failure]);
  }
}

/**
 * The predicted-vs-observed join: `(ModId, normalizeLogText(operation))`
 * against the *grouped* `VerifyOperationDto` — never against raw
 * `Finding`s (already collapsed server-side, `dto/verify.rs`) and never
 * against an ungrouped log line. See `dto/game_log.rs`'s own doc comment
 * for why this join lives here, client-side, rather than as a third
 * command: there is no session-side cache of either side to join
 * server-side.
 */
export function joinPredictedVsObserved(
  operations: readonly VerifyOperationDto[],
  patchFailures: readonly PatchFailureSummaryDto[],
): PredictedVsObserved {
  const predictedOperationsByMod: Map<string, Set<string>> = new Map();
  for (const op of operations) {
    const normalized = normalizeLogText(op.operation);
    const forMod = predictedOperationsByMod.get(op.modId);
    if (forMod) {
      forMod.add(normalized);
    } else {
      predictedOperationsByMod.set(op.modId, new Set([normalized]));
    }
  }

  const observedByMod: ByModThenOperation = new Map();
  const observedOnly: PatchFailureSummaryDto[] = [];

  for (const failure of patchFailures) {
    const normalizedOperation = normalizeLogText(failure.operation);
    const isPredicted =
      failure.attribution.kind === "mod" &&
      (predictedOperationsByMod.get(failure.attribution.modId)?.has(normalizedOperation) ?? false);
    if (!isPredicted || failure.attribution.kind !== "mod") {
      observedOnly.push(failure);
      continue;
    }
    pushObserved(observedByMod, failure.attribution.modId, normalizedOperation, failure);
  }

  const matched: MatchedOperation[] = [];
  const predictedOnly: VerifyOperationDto[] = [];
  for (const operation of operations) {
    const normalizedOperation = normalizeLogText(operation.operation);
    const observed = observedByMod.get(operation.modId)?.get(normalizedOperation);
    if (observed) {
      matched.push({
        modId: operation.modId,
        operation: operation.operation,
        predicted: operation,
        observed,
      });
    } else {
      predictedOnly.push(operation);
    }
  }

  return { matched, predictedOnly, observedOnly };
}
