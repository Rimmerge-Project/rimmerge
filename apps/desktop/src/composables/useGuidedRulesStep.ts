import { useToast } from "primevue/usetoast";
import { computed, ref, watch } from "vue";
import { useTauriEvent } from "@/composables/useTauriEvent";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import {
  useGetRecommendedRulesMutation,
  useRecommendedRulesStepQuery,
  useSkipRecommendedRulesStepMutation,
} from "@/queries/rules";
import type { RecommendedRulesProgressEventDto } from "@/types/generated/RecommendedRulesProgressEventDto";
import type { RecommendedRulesReportDto } from "@/types/generated/RecommendedRulesReportDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import {
  describeRecommendedRulesReport,
  recommendedRulesReportSeverity,
  recommendedRulesReportTechnicalDetail,
  rulesClickLabel,
} from "@/utils/recommendedRules";

/** `rim-session`'s progress event: one tick per source downloaded, then one for the import. */
const PROGRESS_EVENT = "rules://recommended-progress";

/** How long the result toast stays up: a sentence to read, not a confirmation blink. */
const RESULT_TOAST_LIFE_MS = 6000;

/**
 * Step 1 of the Dashboard strip, "Get the recommended rules": the derived step state, the
 * click that runs it, Skip, and the latest progress tick of a running click.
 *
 * `rules` is `null` until the step query has answered (or when it failed), so the strip never
 * blocks on it. While this window's own click is running it reads `inProgress` without waiting
 * for the backend, and a click another window started arrives as the backend's own `inProgress`.
 */
export function useGuidedRulesStep() {
  const tm = useTranslateMessage();
  const toast = useToast();
  const { data, isPending, error } = useRecommendedRulesStepQuery();
  const { mutateAsync: requestRules, isLoading: isGetting } = useGetRecommendedRulesMutation();
  const { mutateAsync: requestSkip, isLoading: isSkipping } = useSkipRecommendedRulesStepMutation();
  const latestProgress = ref<RecommendedRulesProgressEventDto | null>(null);
  const runLabel = ref<MessageDescriptor | null>(null);

  useTauriEvent<RecommendedRulesProgressEventDto>(PROGRESS_EVENT, (payload) => {
    latestProgress.value = payload;
  });

  const rules = computed<RecommendedRulesStepDto | null>(() =>
    isGetting.value ? { kind: "inProgress" } : (data.value ?? null),
  );

  // A tick left over from a run that has ended (another window's, say) must not reappear when
  // the next run starts.
  watch(
    () => rules.value?.kind,
    (kind) => {
      if (kind !== "inProgress") {
        latestProgress.value = null;
      }
    },
  );

  const progress = computed(() =>
    rules.value?.kind === "inProgress" ? latestProgress.value : null,
  );

  /**
   * Runs the click and toasts its result. A rejected run (a closed gate, a damaged settings
   * file, a second concurrent run) was already toasted by the app-wide mutation handler, so
   * nothing more is shown here. The toast is built outside the `try`, so a programming error
   * while describing the report is not swallowed with the rejection.
   */
  async function getRules(): Promise<void> {
    latestProgress.value = null;
    runLabel.value = data.value === undefined ? null : rulesClickLabel(data.value);
    let report: RecommendedRulesReportDto;
    try {
      report = await requestRules();
    } catch {
      // Already reported by `mutationOptions.onError` (see `main.ts`).
      return;
    } finally {
      latestProgress.value = null;
      runLabel.value = null;
    }
    showReport(report);
  }

  function showReport(report: RecommendedRulesReportDto): void {
    const technicalDetail = recommendedRulesReportTechnicalDetail(report);
    toast.add({
      severity: recommendedRulesReportSeverity(report),
      summary: tm(describeRecommendedRulesReport(report)),
      ...(technicalDetail === null
        ? {}
        : { detail: tm(descriptor("common.technicalDetail", { detail: technicalDetail })) }),
      life: RESULT_TOAST_LIFE_MS,
    });
  }

  /** Remembers the skip for this profile; a failure is toasted by the app-wide handler. */
  async function skip(): Promise<void> {
    try {
      await requestSkip();
    } catch {
      // Already reported by `mutationOptions.onError` (see `main.ts`).
    }
  }

  return {
    rules,
    isPending,
    hasLoadFailed: computed(() => error.value !== null && data.value === undefined),
    isSkipping,
    progress,
    runLabel,
    getRules,
    skip,
  };
}
