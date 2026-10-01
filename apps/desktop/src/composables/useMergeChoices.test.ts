import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

import { useMergeChoices } from "@/composables/useMergeChoices";
import { RimmergeError } from "@/services/ipc";
import { installMockIpc } from "@/services/ipc.mock";
import { asFieldPath, asFindingKey, type FindingKey } from "@/types/brands";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { SetMergeChoicesRequestDto } from "@/types/generated/SetMergeChoicesRequestDto";

const ORIGINAL_KEY = asFindingKey("def_override:ThingDef/Wall:[a,b]");

function field(overrides: Partial<MergeFieldDto> = {}): MergeFieldDto {
  return {
    path: "label",
    depth: 0,
    isListItem: false,
    entry: "leaf",
    container: null,
    class: "conflict",
    changedBy: [],
    confidence: 0,
    base: "base value",
    candidates: {},
    result: null,
    choice: null,
    preselected: null,
    ...overrides,
  };
}

/**
 * `respond` defaults to a successful reply; pass a function that throws
 * (e.g. `() => { throw { code: ..., message: ... }; }`, the same shape
 * `services/ipc.ts`'s `call` maps to `RimmergeError`) to simulate a
 * rejected `set_merge_choices`. `keyRef` is exposed so a test can change
 * what the composable's own `key` getter resolves to — simulating the
 * route param it's normally backed by going away — after scheduling a
 * flush but before it fires.
 */
function mountHarness(
  respond: (payload: unknown) => unknown = () => ({ kind: "complete", opCount: 1 }),
  patchId: string | null = null,
) {
  const calls: SetMergeChoicesRequestDto[] = [];
  installMockIpc({
    set_merge_choices: (payload: unknown) => {
      const request = (payload as { request: SetMergeChoicesRequestDto }).request;
      calls.push(request);
      return respond(payload);
    },
  });

  const keyRef = ref<FindingKey>(ORIGINAL_KEY);
  let composable!: ReturnType<typeof useMergeChoices>;
  const Harness = defineComponent({
    setup() {
      composable = useMergeChoices(
        () => keyRef.value,
        () => patchId,
      );
      return {};
    },
    template: "<div />",
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });

  return {
    wrapper,
    calls,
    keyRef,
    get choices() {
      return composable;
    },
  };
}

describe("useMergeChoices", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    clearMocks();
    vi.useRealTimers();
  });

  it("seeds the local map only from fields that already carry a stored choice", () => {
    const { choices, wrapper } = mountHarness();

    choices.seed([
      field({ path: "label", choice: { choice: "from", modId: "a.mod" } }),
      field({ path: "description", choice: null }),
    ]);

    expect(choices.choices.value).toEqual({ label: { choice: "from", modId: "a.mod" } });
    wrapper.unmount();
  });

  it("seeding is skipped while a flush is pending, applied once idle", async () => {
    const { choices, wrapper } = mountHarness();

    // A local edit schedules a flush — seeding must not touch the map
    // until that flush (and any request it sends) has settled.
    const pending = choices.setChoice(asFieldPath("description"), {
      choice: "value",
      text: "draft",
    });
    choices.seed([field({ path: "label", choice: { choice: "from", modId: "a.mod" } })]);
    expect(choices.choices.value).toEqual({ description: { choice: "value", text: "draft" } });

    await vi.advanceTimersByTimeAsync(150);
    await pending;

    // Idle now: seeding applies both directions — adds a path the server
    // reports a choice for, and removes one it no longer does.
    choices.seed([field({ path: "label", choice: { choice: "from", modId: "b.mod" } })]);
    expect(choices.choices.value).toEqual({
      description: { choice: "value", text: "draft" },
      label: { choice: "from", modId: "b.mod" },
    });

    choices.seed([field({ path: "label", choice: null })]);
    expect(choices.choices.value).toEqual({ description: { choice: "value", text: "draft" } });
    wrapper.unmount();
  });

  it("a local remove made while a stale refetch's seed lands mid-flush is preserved and sent", async () => {
    const { choices, calls, wrapper } = mountHarness();

    // Choose, and let that flush land — the server now "knows" about it.
    const chosen = choices.setChoice(asFieldPath("label"), { choice: "from", modId: "a.mod" });
    await vi.advanceTimersByTimeAsync(150);
    await chosen;
    expect(choices.choices.value).toEqual({ label: { choice: "from", modId: "a.mod" } });

    // The user removes it locally — this schedules a new flush.
    const cleared = choices.clearChoice(asFieldPath("label"));

    // A refetch (triggered by the *previous* flush's invalidation) lands
    // mid-debounce, still carrying the now-superseded stored choice.
    choices.seed([field({ path: "label", choice: { choice: "from", modId: "a.mod" } })]);
    expect(choices.choices.value).toEqual({});

    await vi.advanceTimersByTimeAsync(150);
    await cleared;

    expect(choices.choices.value).toEqual({});
    expect(calls.at(-1)?.choices).toEqual({});
    wrapper.unmount();
  });

  it("debounces a burst of setChoice calls into a single request carrying the full map", async () => {
    const { choices, calls, wrapper } = mountHarness();

    const first = choices.setChoice(asFieldPath("label"), { choice: "from", modId: "a.mod" });
    await Promise.resolve();
    const second = choices.setChoice(asFieldPath("description"), { choice: "drop" });

    expect(calls).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(150);

    expect(calls).toHaveLength(1);
    expect(calls[0]?.choices).toEqual({
      label: { choice: "from", modId: "a.mod" },
      description: { choice: "drop" },
    });

    // Both promises settle from the one request, "last write wins".
    await expect(first).resolves.toEqual({ kind: "complete", opCount: 1 });
    await expect(second).resolves.toEqual({ kind: "complete", opCount: 1 });
    wrapper.unmount();
  });

  it("clearChoice removes a path from the map and sends the request", async () => {
    const { choices, calls, wrapper } = mountHarness();
    choices.seed([field({ path: "label", choice: { choice: "from", modId: "a.mod" } })]);

    const pending = choices.clearChoice(asFieldPath("label"));
    await vi.advanceTimersByTimeAsync(150);
    await pending;

    expect(choices.choices.value).toEqual({});
    expect(calls[0]?.choices).toEqual({});
    wrapper.unmount();
  });

  it("unmounting with a pending timer flushes synchronously, sending exactly one request with the key captured at scheduling time", async () => {
    const { choices, calls, wrapper, keyRef } = mountHarness();

    const pending = choices.setChoice(asFieldPath("label"), { choice: "from", modId: "a.mod" });
    // The `key` source (e.g. a route param) has already gone away by the
    // time unmount runs — the send must still use what it was when the
    // pick was made, not this.
    keyRef.value = asFindingKey("");

    wrapper.unmount();
    await pending;

    expect(calls).toHaveLength(1);
    expect(calls[0]?.key).toBe(ORIGINAL_KEY);
    expect(calls[0]?.choices).toEqual({ label: { choice: "from", modId: "a.mod" } });

    // The cancelled debounce timer must never also fire later.
    await vi.advanceTimersByTimeAsync(150);
    expect(calls).toHaveLength(1);
  });

  it("unmounting with no flush pending sends nothing", () => {
    const { choices, calls, wrapper } = mountHarness();
    choices.seed([field({ path: "label", choice: { choice: "from", modId: "a.mod" } })]);

    wrapper.unmount();

    expect(calls).toHaveLength(0);
  });

  it("a rejected set_merge_choices rejects the waiters and leaves the optimistic edit in the local map", async () => {
    const { choices, wrapper } = mountHarness(() => {
      throw { code: "invalid_input", message: "bad field path" };
    });

    const pending = choices.setChoice(asFieldPath("label"), { choice: "from", modId: "a.mod" });
    // The rejection assertion is set up *before* advancing the fake
    // timer that triggers the send, not after: `set_merge_choices`
    // rejects several real microtask hops deep inside Pinia Colada's own
    // mutation pipeline (onMutate → mutation → onError → onSettled), and
    // `vi.advanceTimersByTimeAsync` doesn't flush enough of them for a
    // handler attached only afterwards to count as attached "in time" —
    // Node still fires (a spurious, but real) `unhandledRejection` in
    // that ordering even though `.catch` handling is already wired up
    // inside `sendAndSettle`.
    const rejection = expect(pending).rejects.toBeInstanceOf(RimmergeError);
    await vi.advanceTimersByTimeAsync(150);
    await rejection;

    // Documented behavior (see the composable's own doc comment): a
    // failed send does not roll the local map back.
    expect(choices.choices.value).toEqual({ label: { choice: "from", modId: "a.mod" } });
    wrapper.unmount();
  });

  it("carries patchId on every flush when given, and null otherwise", async () => {
    const { choices, calls, wrapper } = mountHarness(undefined, "patch-1");

    const pending = choices.setChoice(asFieldPath("label"), { choice: "from", modId: "a.mod" });
    await vi.advanceTimersByTimeAsync(150);
    await pending;

    expect(calls[0]?.patchId).toBe("patch-1");
    wrapper.unmount();

    const withoutPatch = mountHarness();
    const pendingWithoutPatch = withoutPatch.choices.setChoice(asFieldPath("label"), {
      choice: "from",
      modId: "a.mod",
    });
    await vi.advanceTimersByTimeAsync(150);
    await pendingWithoutPatch;

    expect(withoutPatch.calls[0]?.patchId).toBeNull();
    withoutPatch.wrapper.unmount();
  });
});
