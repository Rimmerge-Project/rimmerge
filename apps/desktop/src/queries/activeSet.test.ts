import { PiniaColada, useQuery } from "@pinia/colada";
import { invoke } from "@tauri-apps/api/core";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import { EMPTY_PENDING_ACTIVE_CHANGES } from "@/components/apply/ApplyDialog.test-support";
import { useActivateModsMutation } from "@/queries/activeSet";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";

/**
 * A committed activation must stay a success when an unrelated query's refetch fails: Colada
 * awaits `onSuccess` inside `mutate`, so a raw `invalidateQueries` there would flip the mutation
 * to `error`, fire the app-wide `mutationOptions.onError` toast for the wrong operation and
 * reject `mutateAsync`.
 */
describe("useActivateModsMutation", () => {
  afterEach(() => {
    clearMocks();
  });

  it("stays successful, with no error toast, when an unrelated refetch fails", async () => {
    let reads = 0;
    installMockIpc({
      activate_mods: () => EMPTY_PENDING_ACTIVE_CHANGES,
      unrelated_read: () => {
        reads += 1;
        if (reads > 1) {
          throw new Error("refetch failed");
        }
        return "first";
      },
    });
    const onError = vi.fn();
    let mutation!: ReturnType<typeof useActivateModsMutation>;
    const Harness = defineComponent({
      setup() {
        mutation = useActivateModsMutation();
        useQuery({ key: ["unrelated"], query: () => invoke<string>("unrelated_read") });
        return {};
      },
      template: "<div />",
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mount(Harness, {
      global: { plugins: [pinia, [PiniaColada, { mutationOptions: { onError } }]] },
    });
    await flushPromises();

    const request: ActivateRequestDto = { ids: ["author.mod"], withDependencies: false };
    await expect(mutation.mutateAsync(request)).resolves.toEqual(EMPTY_PENDING_ACTIVE_CHANGES);

    expect(reads).toBe(2);
    expect(mutation.status.value).toBe("success");
    expect(onError).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
