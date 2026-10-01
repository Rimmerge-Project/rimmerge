<script setup lang="ts">
import Button from "primevue/button";
import { computed, onMounted, onScopeDispose, provide, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import BaseProgressBar from "@/components/base/BaseProgressBar.vue";
import PendingChangesBanner from "@/components/mods/PendingChangesBanner.vue";
import PendingChangesCloseGuard from "@/components/mods/PendingChangesCloseGuard.vue";
import NotificationBell from "@/components/notifications/NotificationBell.vue";
import { SCROLL_CONTAINER_KEY } from "@/composables/scrollContainer";
import { isInert } from "@/composables/shortcutTargets";
import { useModLabel } from "@/composables/useModLabel";
import { useOrderSource } from "@/composables/useOrderSource";
import { useTauriEvent } from "@/composables/useTauriEvent";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useOpenAppLinkMutation } from "@/queries/links";
import { useNotificationsQuery } from "@/queries/notifications";
import { useSessionStore } from "@/stores/session";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { ProgressEventDto } from "@/types/generated/ProgressEventDto";
import { newArrivals, notificationKeyId } from "@/utils/notificationArrivals";
import { notificationTitle } from "@/utils/notifications";

const { t } = useI18n();
const tm = useTranslateMessage();
const { selected, select, isSwitching } = useOrderSource();
const session = useSessionStore();
const modLabel = useModLabel();
const { mutateAsync: openAppLink } = useOpenAppLinkMutation();

/**
 * A visually hidden `aria-live="polite"` region announcing a genuine new
 * arrival — never a plain repeat of the current count (screen readers
 * don't re-announce an unchanged live region anyway, but "Notifications,
 * 2" twice in a row says nothing about what just arrived either). `watch`
 * (not `watchEffect`) so a decrease (a dismiss/mute) never re-announces,
 * and the first load (`previousKeys === null`) never announces either —
 * only a notice absent from the previous list does. Names the newest
 * arrival's own title rather than every one that arrived at once, so a
 * burst of notices announces once, not a spam of consecutive utterances.
 */
const { data: notifications } = useNotificationsQuery();
const liveAnnouncement = ref("");
let previousKeys: Set<string> | null = null;
watch(
  () => notifications.value,
  (list) => {
    if (!list) {
      return;
    }
    const arrivals = newArrivals(previousKeys, list);
    previousKeys = new Set(list.map(notificationKeyId));
    const newest = arrivals.at(-1);
    if (!newest) {
      return;
    }
    liveAnnouncement.value = t(
      "shell.liveRegion.newNotification",
      { count: arrivals.length, title: tm(notificationTitle(newest)) },
      arrivals.length,
    );
  },
);

function openSupportLink(): void {
  void openAppLink("support");
}

/**
 * `t`: toggles the mod-id-vs-display-name mode from any shell page —
 * unlike the inbox/merge-editor's own shortcuts, this one lives here
 * (not in a page-scoped `useXKeys` composable) because it needs to work
 * everywhere the shell renders, not just on one page. Uses the same
 * `isInert` guard those composables share, from `shortcutTargets.ts`.
 */
function handleGlobalKeydown(event: KeyboardEvent): void {
  if (event.key !== "t" || isInert(event.target)) {
    return;
  }
  event.preventDefault();
  modLabel.toggle();
}
onMounted(() => window.addEventListener("keydown", handleGlobalKeydown));
onScopeDispose(() => window.removeEventListener("keydown", handleGlobalKeydown));

/** The loaded project's game-dir basename, for the brand row — `null` before a project loads (the router never lets that happen here, but the shell shouldn't assume it). */
const projectName = computed(() => {
  const gameDir = session.paths?.gameDir;
  if (!gameDir) {
    return null;
  }
  return gameDir.split(/[/\\]/).findLast((segment) => segment.length > 0) ?? null;
});

const applyDialogVisible = ref(false);

const progress = ref<ProgressEventDto | null>(null);

useTauriEvent<ProgressEventDto>("project://progress", (payload) => {
  progress.value = payload;
});

const ORDER_SOURCES: OrderSourceDto[] = ["current", "suggested"];
const pageScroller = useTemplateRef<HTMLElement>("pageScroller");
// Pages are rendered inside this scroller; thumbnails measure "near" against it.
provide(SCROLL_CONTAINER_KEY, pageScroller);
const currentSourceButton = useTemplateRef<HTMLButtonElement>("currentSourceButton");
const suggestedSourceButton = useTemplateRef<HTMLButtonElement>("suggestedSourceButton");

/**
 * The ARIA `radiogroup` pattern moves both focus and selection together
 * on arrow keys (Left/Up and Right/Down each move to the other option,
 * wrapping since there are only two) — clicking is not the only way in.
 */
function moveOrderSource(event: KeyboardEvent): void {
  if (!["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown"].includes(event.key)) {
    return;
  }
  event.preventDefault();
  const currentIndex = ORDER_SOURCES.indexOf(selected.value);
  const delta = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
  const next = ORDER_SOURCES[(currentIndex + delta + ORDER_SOURCES.length) % ORDER_SOURCES.length];
  if (!next) {
    return;
  }
  select(next);
  (next === "current" ? currentSourceButton : suggestedSourceButton).value?.focus();
}

// A `labelKey` per item, never a translated `label`: this array is
// module-level, built once, so a locale switch would otherwise leave
// the nav frozen in whatever language it was in at first render. The
// template calls `t(item.labelKey)` itself instead, which re-evaluates
// on every render — including the one a locale switch triggers.
const NAV_ITEMS: { to: string; labelKey: string; testId: string; icon: string }[] = [
  { to: "/", labelKey: "shell.nav.dashboard", testId: "nav-dashboard", icon: "pi-home" },
  { to: "/inbox", labelKey: "shell.nav.inbox", testId: "nav-inbox", icon: "pi-inbox" },
  {
    to: "/merge-mod",
    labelKey: "shell.nav.mergeMod",
    testId: "nav-merge-mod",
    icon: "pi-file-export",
  },
  { to: "/patches", labelKey: "shell.nav.patches", testId: "nav-patches", icon: "pi-wrench" },
  {
    to: "/assignments",
    labelKey: "shell.nav.assignments",
    testId: "nav-assignments",
    icon: "pi-sitemap",
  },
  { to: "/startup", labelKey: "shell.nav.startup", testId: "nav-startup", icon: "pi-gauge" },
  { to: "/order", labelKey: "shell.nav.order", testId: "nav-order", icon: "pi-list" },
  { to: "/rules", labelKey: "shell.nav.rules", testId: "nav-rules", icon: "pi-sliders-h" },
  { to: "/mods", labelKey: "shell.nav.mods", testId: "nav-mods", icon: "pi-box" },
  { to: "/settings", labelKey: "shell.nav.settings", testId: "nav-settings", icon: "pi-cog" },
];
</script>

<template>
  <div class="flex h-screen">
    <aside
      class="border-border-subtle bg-surface-1 flex w-56 shrink-0 flex-col gap-4 overflow-y-auto border-r p-3"
      :aria-label="t('shell.mainNavigation')"
    >
      <div class="flex items-start justify-between gap-2 px-1 pt-1">
        <div class="flex flex-col gap-0.5">
          <span class="text-text flex items-center gap-2 text-sm font-semibold tracking-wide">
            <svg viewBox="0 0 100 100" class="size-6 shrink-0" aria-hidden="true">
              <rect width="100" height="100" rx="22" fill="#4F46E5" />
              <g fill="none" stroke="#fff" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="50" cy="50" r="33" stroke-width="7" />
                <path d="M38 31 C38 48 50 48 50 56" stroke-width="7.5" />
                <path d="M62 31 C62 48 50 48 50 56" stroke-width="7.5" />
                <path d="M50 56 V66" stroke-width="7.5" />
                <path d="M42 61 L50 70 L58 61" stroke-width="7.5" />
              </g>
            </svg>
            {{ t("shell.brand") }}
          </span>
          <span
            v-if="projectName"
            class="text-text-faint truncate text-xs"
            :title="session.paths?.gameDir"
          >
            {{ projectName }}
          </span>
        </div>
        <NotificationBell />
      </div>

      <nav>
        <ul class="flex flex-col gap-1">
          <li
            v-for="item in NAV_ITEMS"
            :key="item.to"
          >
            <RouterLink
              :to="item.to"
              class="text-text-muted hover:bg-surface-2 hover:text-text flex items-center gap-2 rounded px-2 py-1.5 text-sm focus-visible:outline"
              exact-active-class="bg-accent-soft text-accent font-semibold hover:bg-accent-soft"
              :data-testid="item.testId"
            >
              <i
                class="pi text-sm"
                :class="item.icon"
                aria-hidden="true"
              />
              {{ t(item.labelKey) }}
            </RouterLink>
          </li>
        </ul>
      </nav>

      <div class="border-border-subtle mt-auto flex flex-col gap-2 border-t pt-3">
        <div
          class="bg-surface-2 flex overflow-hidden rounded text-xs"
          role="radiogroup"
          :aria-label="t('shell.orderSource.label')"
          @keydown="moveOrderSource"
        >
          <button
            ref="currentSourceButton"
            type="button"
            class="flex-1 cursor-pointer px-2 py-1.5 font-medium transition-colors focus-visible:outline"
            :class="
              selected === 'current'
                ? 'bg-accent text-on-accent'
                : 'text-text-muted hover:bg-surface-1 hover:text-text'
            "
            role="radio"
            :aria-checked="selected === 'current'"
            :tabindex="selected === 'current' ? 0 : -1"
            :disabled="isSwitching"
            data-testid="order-source-current"
            @click="select('current')"
          >
            {{ t("shell.orderSource.current") }}
          </button>
          <button
            ref="suggestedSourceButton"
            type="button"
            class="flex-1 cursor-pointer px-2 py-1.5 font-medium transition-colors focus-visible:outline"
            :class="
              selected === 'suggested'
                ? 'bg-accent text-on-accent'
                : 'text-text-muted hover:bg-surface-1 hover:text-text'
            "
            role="radio"
            :aria-checked="selected === 'suggested'"
            :tabindex="selected === 'suggested' ? 0 : -1"
            :disabled="isSwitching"
            data-testid="order-source-suggested"
            @click="select('suggested')"
          >
            {{ t("shell.orderSource.suggested") }}
          </button>
        </div>

        <div
          class="bg-surface-2 flex overflow-hidden rounded text-xs"
          role="radiogroup"
          :aria-label="t('shell.modLabelMode.label')"
          data-testid="mod-label-mode"
        >
          <button
            type="button"
            class="flex-1 cursor-pointer px-2 py-1.5 font-medium transition-colors focus-visible:outline"
            :class="
              modLabel.mode.value === 'name'
                ? 'bg-accent text-on-accent'
                : 'text-text-muted hover:bg-surface-1 hover:text-text'
            "
            role="radio"
            :aria-checked="modLabel.mode.value === 'name'"
            :tabindex="modLabel.mode.value === 'name' ? 0 : -1"
            data-testid="mod-label-mode-name"
            @click="modLabel.mode.value !== 'name' && modLabel.toggle()"
          >
            {{ t("shell.modLabelMode.names") }}
          </button>
          <button
            type="button"
            class="flex-1 cursor-pointer px-2 py-1.5 font-medium transition-colors focus-visible:outline"
            :class="
              modLabel.mode.value === 'id'
                ? 'bg-accent text-on-accent'
                : 'text-text-muted hover:bg-surface-1 hover:text-text'
            "
            role="radio"
            :aria-checked="modLabel.mode.value === 'id'"
            :tabindex="modLabel.mode.value === 'id' ? 0 : -1"
            data-testid="mod-label-mode-id"
            @click="modLabel.mode.value !== 'id' && modLabel.toggle()"
          >
            {{ t("shell.modLabelMode.ids") }}
          </button>
        </div>

        <Button
          :label="t('shell.apply')"
          fluid
          data-testid="shell-apply-button"
          @click="applyDialogVisible = true"
        />

        <button
          type="button"
          class="text-text-faint hover:text-text-muted cursor-pointer text-center text-xs underline-offset-2 hover:underline"
          data-testid="shell-support-link"
          @click="openSupportLink"
        >
          {{ t("shell.supportLink") }}
        </button>
      </div>
    </aside>

    <div class="flex flex-1 flex-col overflow-hidden">
      <BaseProgressBar
        v-if="progress && progress.total > 0 && progress.done < progress.total"
        :progress="{ done: progress.done, total: progress.total }"
        :label="t('shell.scanningMods')"
        data-testid="shell-progress"
      />
      <PendingChangesBanner />
      <main
        ref="pageScroller"
        class="bg-surface-0 min-h-0 flex-1 overflow-y-auto"
      >
        <RouterView />
      </main>
    </div>

    <ApplyDialog v-model:visible="applyDialogVisible" />
    <PendingChangesCloseGuard />
    <div
      role="status"
      aria-live="polite"
      class="sr-only"
      data-testid="notification-live-region"
    >
      {{ liveAnnouncement }}
    </div>
  </div>
</template>
