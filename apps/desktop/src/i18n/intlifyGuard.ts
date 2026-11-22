import { afterAll, afterEach, expect } from "vitest";

/**
 * vue-i18n reports misuse (a `<i18n-t>` with no parent scope, a key missing from a catalogue, a
 * fallback to another locale) only through `console.warn("[intlify] ...")`. That line is easy to
 * miss: a green run does not fail on it, and some reporters (the minimal one vitest picks in a
 * non-interactive run) show no console output of a passing test at all. So a test that provokes
 * one fails.
 *
 * How it works: the wrapper around `console.warn` is installed once per test file, at module
 * scope of the setup file, and records each `[intlify]` message with the name of the test that was
 * running. `afterEach` throws for the finished test's warnings; `afterAll` throws for any left over,
 * such as a warning a timer or an unawaited promise raised after its test ended (those are blamed on
 * the last test that was current, or on the file when none was). The wrapper forwards every message,
 * so other warnings still print.
 *
 * Limits: a warning raised after the file's last hook has run (a timer with a longer delay) is
 * not seen at all. A test that replaces `console.warn` itself (`vi.spyOn(console, "warn")` with a
 * `mockImplementation` that does not call through) hides intlify warnings from the guard for as
 * long as the spy is installed, because the guard only sees what reaches its wrapper. A warning
 * that lands while a later test runs is blamed on that test. A test that provokes warnings on
 * purpose, such as one exercising the fallback chain, calls {@link allowIntlifyWarnings}.
 */
const INTLIFY_PREFIX = "[intlify]";

/** The name `expect` reports when no test is running (a warning raised outside any test). */
const NO_TEST = "(outside any test)";

interface RecordedWarning {
  readonly test: string;
  readonly message: string;
}

let pending: RecordedWarning[] = [];
const allowedTests = new Set<string>();

function currentTest(): string {
  return expect.getState().currentTestName ?? NO_TEST;
}

/**
 * Lets the current test provoke vue-i18n warnings on purpose (a test of the fallback chain itself).
 * Call it inside the test or its `beforeEach`; the allowance covers only that test.
 */
export function allowIntlifyWarnings(): void {
  allowedTests.add(currentTest());
}

function describeWarnings(warnings: readonly RecordedWarning[]): string {
  return warnings.map(({ test, message }) => `${message} (in "${test}")`).join(" | ");
}

/** Takes the not-allowed warnings out of `pending` that `isWanted` selects. */
function drain(isWanted: (warning: RecordedWarning) => boolean): RecordedWarning[] {
  const taken = pending.filter(isWanted);
  pending = pending.filter((warning) => !isWanted(warning));
  return taken.filter((warning) => !allowedTests.has(warning.test));
}

/**
 * Wraps `console.warn` and registers the checks. Called once per file from `test-setup.ts`.
 * Calling it a second time in the same file would stack a second wrapper, so it never is.
 */
export function installIntlifyGuard(): void {
  const forwardTo = console.warn;
  console.warn = (...args: unknown[]) => {
    const [first] = args;
    if (typeof first === "string" && first.startsWith(INTLIFY_PREFIX)) {
      pending.push({ test: currentTest(), message: first });
    }
    forwardTo.apply(console, args);
  };

  afterEach(() => {
    const own = drain((warning) => warning.test === currentTest());
    if (own.length > 0) {
      throw new Error(`vue-i18n warned during the test: ${describeWarnings(own)}`);
    }
  });

  afterAll(async () => {
    // One macrotask, so a zero-delay timer a test left behind still lands in `pending`.
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    const late = drain(() => true);
    if (late.length > 0) {
      throw new Error(`vue-i18n warned after its test had finished: ${describeWarnings(late)}`);
    }
  });
}
