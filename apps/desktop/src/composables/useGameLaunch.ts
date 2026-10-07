import { useToast } from "primevue/usetoast";
import { computed, nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { ApplyOutcome } from "@/composables/useApplyDialog";
import { useGameLaunchPolling } from "@/composables/useGameLaunchPolling";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import { useGameLaunchStatusQuery } from "@/queries/gameLaunch";
import { launchGame, RimmergeError } from "@/services/ipc";
import { useGameLaunchStore } from "@/stores/gameLaunch";
import { useSessionStore } from "@/stores/session";
import type { GameLaunchRouteDto } from "@/types/generated/GameLaunchRouteDto";
import type { GameLaunchStatusDto } from "@/types/generated/GameLaunchStatusDto";
import type { IfNotAppliedDto } from "@/types/generated/IfNotAppliedDto";
import type { UnappliedReasonDto } from "@/types/generated/UnappliedReasonDto";
import { assertNever } from "@/utils/assertNever";
import { describeCommandError } from "@/utils/errors";
import {
  applyFirstSentence,
  type GameLaunchView,
  gameLaunchedLabel,
  gameLaunchHint,
  gameLaunchLabel,
  gameLaunchStartingLabel,
  isGameLaunchClickable,
} from "@/utils/gameLaunch";

const TOAST_LIFE_MS = 6000;

/**
 * The Launch RimWorld button's state and actions: the polled status, the click (always
 * against a fresh status), the Apply-first prompt, the Apply-then-launch hand-off and the
 * "Starting…" window.
 *
 * Launch errors are handled here, not through a mutation: the app-wide mutation handler
 * toasts every failure with its generic sentence, which is wrong for `rimworld_running`
 * ("or continue anyway" does not apply) and `order_not_applied` (it reopens the prompt).
 * A failure is therefore shown exactly once, by this composable.
 *
 * What is in flight, and the Starting window, live in `useGameLaunchStore`, shared by every
 * button, so a click on one disables the others. Only one instance (`pollsStatus`, the
 * always-mounted sidebar) polls; the others read the same query.
 */
export function useGameLaunch(options: { readonly pollsStatus: boolean }) {
  const { t } = useI18n();
  const tm = useTranslateMessage();
  const toast = useToast();
  const session = useSessionStore();
  const query = useGameLaunchStatusQuery();
  const launchState = useGameLaunchStore();
  if (options.pollsStatus) {
    useGameLaunchPolling(query);
  }

  /** The reason the Apply-first prompt is asking about; `null` while it is closed. */
  const promptReason = ref<UnappliedReasonDto | null>(null);
  const isApplyOpen = ref(false);
  const isLaunchPendingApply = ref(false);
  const announcement = ref("");

  /** A failed query is "unknown" even with an older answer held; an unresolved one only before the first answer. */
  const view = computed<GameLaunchView>(() => {
    if (query.error.value !== null) {
      return { kind: "unknown", hasFailed: true };
    }
    const status = query.data.value;
    return status === undefined ? { kind: "unknown", hasFailed: false } : { kind: "known", status };
  });

  watch(view, (current) => {
    if (current.kind === "known" && current.status.kind === "gameRunning") {
      launchState.endStarting();
    }
  });

  const label = computed(() =>
    tm(launchState.isStarting ? gameLaunchStartingLabel() : gameLaunchLabel(view.value)),
  );
  const hint = computed(() => {
    const described = gameLaunchHint(view.value);
    return described === null ? null : tm(described);
  });
  const isDisabled = computed(() => launchState.isBusy || !isGameLaunchClickable(view.value));
  const promptSentence = computed(() =>
    promptReason.value === null ? "" : tm(applyFirstSentence(promptReason.value, session.selected)),
  );
  const isPromptOpen = computed({
    get: () => promptReason.value !== null,
    set: (isOpen: boolean) => {
      if (!isOpen) {
        promptReason.value = null;
      }
    },
  });

  async function readFreshStatus(): Promise<GameLaunchStatusDto | null> {
    const state = await query.refetch();
    return state.error === null ? (state.data ?? null) : null;
  }

  function openPrompt(reason: UnappliedReasonDto): void {
    promptReason.value = reason;
  }

  /** The button's click: decide from a fresh status, never from the one on screen. */
  async function request(): Promise<void> {
    if (launchState.isBusy) {
      return;
    }
    launchState.isRequesting = true;
    let fresh: GameLaunchStatusDto | null;
    try {
      fresh = await readFreshStatus();
    } finally {
      launchState.isRequesting = false;
    }
    if (fresh === null) {
      return;
    }
    switch (fresh.kind) {
      case "ready":
        await launch("refuse");
        return;
      case "needsApply":
        openPrompt(fresh.reason);
        return;
      case "gameRunning":
      case "unavailable":
        return;
      default:
        assertNever(fresh);
    }
  }

  async function launch(ifNotApplied: IfNotAppliedDto): Promise<void> {
    let route: GameLaunchRouteDto;
    launchState.isRequesting = true;
    try {
      route = (await launchGame({ ifNotApplied })).route;
    } catch (error: unknown) {
      await handleLaunchFailure(error);
      return;
    } finally {
      launchState.isRequesting = false;
    }
    handleLaunched(route);
  }

  function handleLaunched(route: GameLaunchRouteDto): void {
    const sentence = tm(gameLaunchedLabel(route));
    toast.add({ severity: "success", summary: sentence, life: TOAST_LIFE_MS });
    // Cleared first so a repeat launch changes the live region and is spoken again.
    announcement.value = "";
    void nextTick(() => {
      announcement.value = sentence;
    });
    launchState.beginStarting();
  }

  async function handleLaunchFailure(error: unknown): Promise<void> {
    if (error instanceof RimmergeError && error.code === "rimworld_running") {
      toast.add({ severity: "warn", summary: t("gameLaunch.alreadyRunning"), life: TOAST_LIFE_MS });
      await readFreshStatus();
      return;
    }
    if (error instanceof RimmergeError && error.code === "order_not_applied") {
      // The status said Ready, then a decision moved the order: ask again, never launch silently.
      const fresh = await readFreshStatus();
      if (fresh?.kind === "needsApply") {
        openPrompt(fresh.reason);
        return;
      }
    }
    showFailure(error);
  }

  function showFailure(error: unknown): void {
    const described = describeCommandError(error);
    const lines = [tm(described.detail)];
    if (described.technicalDetail !== null) {
      lines.push(tm(descriptor("common.technicalDetail", { detail: described.technicalDetail })));
    }
    toast.add({
      severity: "error",
      summary: tm(described.title),
      detail: lines.join("\n"),
      life: TOAST_LIFE_MS,
    });
  }

  function applyFirst(): void {
    promptReason.value = null;
    isLaunchPendingApply.value = true;
    isApplyOpen.value = true;
  }

  function launchAnyway(): void {
    promptReason.value = null;
    // `launch` reports every failure itself, so nothing is left to await or catch here.
    void launch("launchAnyway");
  }

  function cancelPrompt(): void {
    promptReason.value = null;
  }

  /** `ApplyDialog`'s `applied`: launches only for the Apply this component's own prompt opened. */
  function handleApplied(outcome: ApplyOutcome): void {
    if (!isLaunchPendingApply.value) {
      return;
    }
    isLaunchPendingApply.value = false;
    if (outcome.wroteModsConfig) {
      void launch("refuse");
      return;
    }
    toast.add({
      severity: "info",
      summary: t("gameLaunch.notStartedAfterApply"),
      life: TOAST_LIFE_MS,
    });
  }

  // The dialog closed without applying: the launch the prompt promised is off.
  watch(isApplyOpen, (isOpen) => {
    if (!isOpen) {
      isLaunchPendingApply.value = false;
    }
  });

  return {
    label,
    hint,
    isDisabled,
    isBusy: computed(() => launchState.isBusy),
    isPromptOpen,
    promptSentence,
    isApplyOpen,
    announcement,
    request,
    applyFirst,
    launchAnyway,
    cancelPrompt,
    handleApplied,
  };
}
