import { useTimeoutFn } from "@vueuse/core";
import { defineStore } from "pinia";
import { computed, ref } from "vue";

/** How long "Starting RimWorld…" waits for the game to show up before giving up. */
export const STARTING_WINDOW_MS = 30_000;

/**
 * App-wide state of a launch in progress, shared by every Launch RimWorld button (the sidebar's
 * and the strip's), so a click on one disables the other and a second `steam://run` cannot be
 * sent while the first is still starting. UI state only: the game's own state is the polled
 * status query, never stored here.
 */
export const useGameLaunchStore = defineStore("gameLaunch", () => {
  /** A click's fresh-status read or its `launch_game` call is in flight. */
  const isRequesting = ref(false);
  /** From a successful launch until the status reads `gameRunning` or the window ends. */
  const isStarting = ref(false);
  const startingTimer = useTimeoutFn(
    () => {
      isStarting.value = false;
    },
    STARTING_WINDOW_MS,
    { immediate: false },
  );
  const isBusy = computed(() => isRequesting.value || isStarting.value);

  function beginStarting(): void {
    isStarting.value = true;
    startingTimer.start();
  }

  function endStarting(): void {
    isStarting.value = false;
    startingTimer.stop();
  }

  return { isRequesting, isStarting, isBusy, beginStarting, endStarting };
});
