/**
 * Exhaustiveness helper for a `switch` over a discriminated union: put
 * `default: assertNever(value)` as the final arm so an unhandled variant
 * fails to compile (TypeScript can't narrow `value` to `never`) instead
 * of silently falling through at runtime.
 */
export function assertNever(value: never): never {
  throw new Error(`unreachable: unhandled variant ${JSON.stringify(value)}`);
}
