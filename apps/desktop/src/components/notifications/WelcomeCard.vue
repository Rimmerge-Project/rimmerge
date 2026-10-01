<script setup lang="ts">
// The first-run notice, as a Dashboard section: a prominent, visible
// explanation of the network features, not only a bell entry — the
// card is what makes dismissing it ("Keep these settings") an informed
// choice rather than a dark pattern.
import Button from "primevue/button";
import Message from "primevue/message";
import ToggleSwitch from "primevue/toggleswitch";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useNotificationActions } from "@/composables/useNotificationActions";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useNotificationsQuery } from "@/queries/notifications";
import { useAppSettingsQuery, useUpdateAppSettingsMutation } from "@/queries/settings";
import type { NetworkPolicyDto } from "@/types/generated/NetworkPolicyDto";
import type { NotificationActionDto } from "@/types/generated/NotificationActionDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import type { WelcomeDto } from "@/types/generated/WelcomeDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

const { t } = useI18n();
const { data: notifications } = useNotificationsQuery();
const { data: appSettings } = useAppSettingsQuery();
const { runAction } = useNotificationActions();
const tm = useTranslateMessage();
const { mutateAsync: saveAppSettings, isLoading: isSaving } = useUpdateAppSettingsMutation();

const saveError = ref<CommandErrorDescriptor | null>(null);
const isAnswering = ref(false);

// One settings write at a time, and no answer while one is in flight: an answer pins the
// stored settings (`complete_welcome`), and a pin racing a toggle's write could overwrite
// the user's choice on a fresh install.
const isBusy = computed(() => isSaving.value || isAnswering.value);

const welcome = computed(() => {
  const notification = notifications.value?.find((n) => n.data.kind === "welcome");
  return notification && notification.data.kind === "welcome"
    ? { notification, data: notification.data as WelcomeDto }
    : null;
});

/**
 * Builds the save from `useAppSettingsQuery`'s own data — **never**
 * `welcome.data.network`, which is only a snapshot from whenever the
 * Welcome notice last evaluated and goes stale the moment one switch is
 * saved (the next switch's toggle would otherwise silently resurrect
 * whatever that snapshot still held, including flipping an
 * already-saved change back). Throws rather than silently dropping the
 * save when settings haven't loaded yet — this button is only ever
 * enabled once the card itself has rendered from data that already
 * required a successful `get_app_settings`, so an unloaded state here
 * would mean the query regressed, not a normal race to swallow. A failed
 * save is shown on the card.
 */
async function toggleNetwork(field: keyof NetworkPolicyDto, value: boolean): Promise<void> {
  if (!welcome.value) return;
  if (!appSettings.value) {
    throw new Error("cannot save a network setting: app settings have not loaded yet");
  }
  saveError.value = null;
  try {
    await saveAppSettings({
      ...appSettings.value.settings,
      network: { ...appSettings.value.settings.network, [field]: value },
    });
  } catch (error: unknown) {
    saveError.value = describeCommandError(error);
  }
}

/** Runs one of the card's answer buttons with every control locked until it settles. */
async function answer(action: NotificationActionDto, notification: NotificationDto): Promise<void> {
  if (isBusy.value) return;
  isAnswering.value = true;
  try {
    await runAction(action, notification);
  } finally {
    isAnswering.value = false;
  }
}
</script>

<template>
  <section
    v-if="welcome"
    aria-labelledby="welcome-card-heading"
    class="border-border-subtle bg-surface-1 flex flex-col gap-4 rounded border p-4"
    data-testid="welcome-card"
  >
    <h2
      id="welcome-card-heading"
      class="text-text text-base font-semibold"
    >
      {{ t("notifications.welcome.title") }}
    </h2>
    <p class="text-text-muted text-sm whitespace-pre-line">
      {{ t("notifications.welcome.body") }}
    </p>

    <div class="flex flex-col gap-2">
      <label class="flex items-center justify-between gap-4 text-sm">
        <span class="text-text">{{ t("settings.network.checkForUpdates.label") }}</span>
        <ToggleSwitch
          :model-value="welcome.data.network.checkForUpdates"
          class="shrink-0"
          :disabled="isBusy"
          data-testid="welcome-card-check-for-updates"
          @update:model-value="(value: boolean) => toggleNetwork('checkForUpdates', value)"
        />
      </label>
      <label class="flex items-center justify-between gap-4 text-sm">
        <span class="text-text">{{ t("settings.network.autoRefreshRuleDatabases.label") }}</span>
        <ToggleSwitch
          :model-value="welcome.data.network.autoRefreshRuleDatabases"
          class="shrink-0"
          :disabled="isBusy"
          data-testid="welcome-card-auto-refresh"
          @update:model-value="(value: boolean) => toggleNetwork('autoRefreshRuleDatabases', value)"
        />
      </label>
      <label class="flex items-center justify-between gap-4 text-sm">
        <span class="text-text">{{ t("notifications.welcome.steamWorkshopLabel") }}</span>
        <ToggleSwitch
          :model-value="welcome.data.network.fetchSteamWorkshop"
          class="shrink-0"
          :disabled="isBusy"
          data-testid="welcome-card-steam-workshop"
          @update:model-value="(value: boolean) => toggleNetwork('fetchSteamWorkshop', value)"
        />
      </label>
      <label class="flex items-center justify-between gap-4 text-sm">
        <span class="text-text font-medium">{{ t("settings.network.allowNetwork.label") }}</span>
        <ToggleSwitch
          :model-value="welcome.data.network.allowNetwork"
          class="shrink-0"
          :disabled="isBusy"
          data-testid="welcome-card-allow-network"
          @update:model-value="(value: boolean) => toggleNetwork('allowNetwork', value)"
        />
      </label>
    </div>

    <Message
      v-if="saveError"
      severity="error"
      data-testid="welcome-card-save-error"
    >
      <div class="font-medium">
        {{ tm(saveError.title) }}
      </div>
      <div>{{ tm(saveError.detail) }}</div>
    </Message>

    <p
      v-if="welcome.data.settingsMatchRecommended"
      class="text-text-faint text-xs"
      data-testid="welcome-card-settings-already-recommended"
    >
      {{ t("notifications.welcome.settingsAlreadyRecommended") }}
    </p>

    <div class="flex flex-wrap items-center gap-2">
      <!-- "Keep these settings" and "Turn off internet access" are
           deliberately equal in visual weight (same severity, neither
           outlined nor text-only) — the privacy choice must never read
           as a dark pattern. -->
      <Button
        :label="t('notifications.actions.keepNetworkSettings')"
        data-testid="welcome-card-keep"
        :disabled="isBusy"
        @click="answer('keepNetworkSettings', welcome.notification)"
      />
      <Button
        :label="t('notifications.actions.turnOffNetwork')"
        data-testid="welcome-card-turn-off"
        :disabled="isBusy"
        @click="answer('turnOffNetwork', welcome.notification)"
      />
      <Button
        v-if="!welcome.data.settingsMatchRecommended"
        :label="t('notifications.actions.applyRecommendedSettings')"
        severity="secondary"
        outlined
        data-testid="welcome-card-apply-recommended"
        :disabled="isBusy"
        @click="answer('applyRecommendedSettings', welcome.notification)"
      />
    </div>
  </section>
</template>
