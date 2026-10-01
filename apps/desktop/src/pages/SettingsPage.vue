<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import InputNumber from "primevue/inputnumber";
import Message from "primevue/message";
import ToggleSwitch from "primevue/toggleswitch";
import { computed, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import AboutSection from "@/components/settings/AboutSection.vue";
import LanguagePicker from "@/components/settings/LanguagePicker.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import {
  useCheckForUpdateMutation,
  useMutedNotificationKindsQuery,
  useUnmuteNotificationKindMutation,
} from "@/queries/notifications";
import {
  useAppSettingsQuery,
  useDefaultSettingsMutation,
  useResetNetworkPolicyMutation,
  useSetSettingsMutation,
  useSettingsQuery,
  useUpdateAppSettingsMutation,
} from "@/queries/settings";
import type { CheckForUpdateOutcomeDto } from "@/types/generated/CheckForUpdateOutcomeDto";
import type { NotificationKindDto } from "@/types/generated/NotificationKindDto";
import type { SettingsDto } from "@/types/generated/SettingsDto";
import {
  checkForUpdateTechnicalDetail,
  describeCheckForUpdateOutcome,
  notificationKindLabel,
} from "@/utils/notifications";
import { toastInfo } from "@/utils/toast";

const { t, locale } = useI18n();
const tm = useTranslateMessage();

const { data } = useSettingsQuery();
const { mutate: save, isLoading } = useSetSettingsMutation();
const { mutateAsync: fetchDefaultSettings, isLoading: loadingDefaults } =
  useDefaultSettingsMutation();

const { data: appSettings, isPending: appSettingsPending } = useAppSettingsQuery();
const { mutate: saveAppSettings } = useUpdateAppSettingsMutation();
const { mutateAsync: restoreNetworkDefaults, isLoading: restoringNetworkDefaults } =
  useResetNetworkPolicyMutation();

/**
 * Whether `<base>/app-settings.json` existed but couldn't be read,
 * parsed, or carried a recognized schema — every network switch is then
 * off until the user saves settings again, per
 * `AppSettingsLoadStatusDto`'s own doc comment.
 */
const appSettingsRecovered = computed(() => appSettings.value?.loadStatus === "recovered");

function toggleAllowNetwork(value: boolean): void {
  if (!appSettings.value) return;
  saveAppSettings({
    ...appSettings.value.settings,
    network: { ...appSettings.value.settings.network, allowNetwork: value },
  });
}

function toggleAutoRefreshRuleDatabases(value: boolean): void {
  if (!appSettings.value) return;
  saveAppSettings({
    ...appSettings.value.settings,
    network: { ...appSettings.value.settings.network, autoRefreshRuleDatabases: value },
  });
}

function toggleCheckForUpdates(value: boolean): void {
  if (!appSettings.value) return;
  saveAppSettings({
    ...appSettings.value.settings,
    network: { ...appSettings.value.settings.network, checkForUpdates: value },
  });
}

// Cleared the moment a new check starts, so a stale result from a
// previous click never lingers next to a fresh one still in flight —
// the same convention `RuleDatabasesCard.vue`'s own `lastRefreshResults`
// follows.
const lastCheckResult = ref<CheckForUpdateOutcomeDto | null>(null);
const { mutateAsync: runCheckForUpdate, isLoading: checkingForUpdate } =
  useCheckForUpdateMutation();

async function checkForUpdateNow(): Promise<void> {
  lastCheckResult.value = null;
  lastCheckResult.value = await runCheckForUpdate();
}

const { data: mutedKinds } = useMutedNotificationKindsQuery();
const { mutateAsync: unmuteNotificationKind } = useUnmuteNotificationKindMutation();

// Per-row, not a single shared flag — the mutation's own `isLoading`
// would spin every "Unmute" button in the list for whichever kind is
// currently in flight, not just the one clicked.
const unmutingKind = ref<NotificationKindDto | null>(null);
async function unmuteKind(kind: NotificationKindDto): Promise<void> {
  unmutingKind.value = kind;
  try {
    await unmuteNotificationKind(kind);
  } finally {
    unmutingKind.value = null;
  }
}

// Failure already surfaces through `main.ts`'s global Pinia Colada
// `onError` toast — this only confirms success, the same convention
// every other explicit user action's toast in this app follows.
async function restoreNetworkDefaultsClick(): Promise<void> {
  await restoreNetworkDefaults();
  toastInfo(t("settings.network.restoreDefaultsToast"));
}

// `null` is a real, distinct state here — PrimeVue's `InputNumber` emits
// it while the field is empty (the user cleared it, or hasn't typed
// anything yet), not just before the first load. `submit` refuses to
// save while it's null rather than silently coercing to some default.
const threshold = ref<number | null>(null);
const enforceSoft = ref(false);
const enforceAwareness = ref(false);
const suggestMergeWhenClean = ref(true);
// The sort-evidence controls. Keeping the loaded values as the `ref`
// defaults (rather than hardcoding one) means a save from this page
// before the query resolves never silently resets a value to something
// other than what's on disk. `useImportedPairs` starts `true` to match
// `Settings::default()` — the pre-load flash must never show the
// opposite of what a fresh profile actually has.
const tieBreak = ref<SettingsDto["tieBreak"]>("rebuild");
const useImportedPairs = ref(true);
const useImportedPlacements = ref(true);
// Whether `Layer::Inferred` engine edges (`PatchRemovedNode`/
// `RetextureAfterOwner`/`DefOverrideAfterOrigin`/`PatchRemovedNodeCosmetic`)
// are enforced. `true` matches `EnforcedLayers`'s own default — unlike
// `enforceSoft`/`enforceAwareness`, which stay `false` here for the same
// reason.
const enforceInferred = ref(true);
// Off by default — see `show_dangling_def_references`'s own doc comment
// in `rim-session::Settings` for why.
const showDanglingDefReferences = ref(false);

/**
 * Applies `settings` into the form's own refs, without saving —
 * shared by the initial load sync below and "Reset to defaults", so the
 * two can never drift apart.
 */
function applySettings(settings: SettingsDto): void {
  threshold.value = settings.threshold;
  enforceSoft.value = settings.enforceSoft;
  enforceAwareness.value = settings.enforceAwareness;
  suggestMergeWhenClean.value = settings.suggestMergeWhenClean;
  tieBreak.value = settings.tieBreak;
  useImportedPairs.value = settings.useImportedPairs;
  useImportedPlacements.value = settings.useImportedPlacements;
  enforceInferred.value = settings.enforceInferred;
  showDanglingDefReferences.value = settings.showDanglingDefReferences;
}

const TIE_BREAKS: SettingsDto["tieBreak"][] = ["rebuild", "preserveCurrent"];
const rebuildButton = useTemplateRef<HTMLButtonElement>("rebuildButton");
const preserveCurrentButton = useTemplateRef<HTMLButtonElement>("preserveCurrentButton");

/**
 * The ARIA `radiogroup` pattern moves both focus and selection together
 * on arrow keys — same shape as `TheShell.vue`'s order-source/mod-label
 * controls.
 */
function moveTieBreak(event: KeyboardEvent): void {
  if (!["ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown"].includes(event.key)) {
    return;
  }
  event.preventDefault();
  const currentIndex = TIE_BREAKS.indexOf(tieBreak.value);
  const delta = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
  const next = TIE_BREAKS[(currentIndex + delta + TIE_BREAKS.length) % TIE_BREAKS.length];
  if (!next) {
    return;
  }
  tieBreak.value = next;
  (next === "rebuild" ? rebuildButton : preserveCurrentButton).value?.focus();
}

// Copies the loaded settings into the form exactly once — a background
// refetch (e.g. the `session://changed` listener, or another mutation's
// blanket `invalidateQueries()`) must never clobber an edit the user has
// in progress. `data` starts `undefined` and flips to the settings
// object on first load, so a plain boolean latch is enough; no need for
// a per-field dirty flag since the whole form is seeded together.
// `immediate: true` matters here: a remount onto an already-cached query
// (e.g. navigating away from `/settings` and back) hands `data` a value
// that is *already* current the moment this runs, with no further change
// ever coming — a non-immediate `watch` would silently never fire and
// leave the form stuck at its initial defaults.
let synced = false;
watch(
  data,
  (settings) => {
    if (synced || !settings) {
      return;
    }
    synced = true;
    applySettings(settings);
  },
  { immediate: true },
);

/**
 * Whether the form differs from the last-saved settings — derived from
 * the two, never a hand-toggled flag, so it can't drift from what's
 * actually on screen. `false` before the query resolves or while the
 * threshold field is empty (nothing on disk yet to compare against).
 * This is what tells the user a "Reset to defaults" (or any other edit)
 * hasn't been saved yet.
 */
const isDirty = computed(() => {
  const saved = data.value;
  if (!saved || threshold.value === null) {
    return false;
  }
  return (
    threshold.value !== saved.threshold ||
    enforceSoft.value !== saved.enforceSoft ||
    enforceAwareness.value !== saved.enforceAwareness ||
    suggestMergeWhenClean.value !== saved.suggestMergeWhenClean ||
    tieBreak.value !== saved.tieBreak ||
    useImportedPairs.value !== saved.useImportedPairs ||
    useImportedPlacements.value !== saved.useImportedPlacements ||
    enforceInferred.value !== saved.enforceInferred ||
    showDanglingDefReferences.value !== saved.showDanglingDefReferences
  );
});

const resetConfirmVisible = ref(false);

/**
 * Refills the form from `rim_session::Settings::default()` (never the
 * imported-pairs/placements toggles' own network settings — this reads
 * the backend's own default table, so it can't drift from it). Nothing
 * is saved until the user presses Save — {@link isDirty} is what shows
 * that.
 */
async function confirmReset(): Promise<void> {
  const defaults = await fetchDefaultSettings();
  applySettings(defaults);
  resetConfirmVisible.value = false;
}

function submit(): void {
  if (threshold.value === null) {
    return;
  }
  save({
    threshold: threshold.value,
    enforceSoft: enforceSoft.value,
    enforceAwareness: enforceAwareness.value,
    suggestMergeWhenClean: suggestMergeWhenClean.value,
    tieBreak: tieBreak.value,
    useImportedPairs: useImportedPairs.value,
    useImportedPlacements: useImportedPlacements.value,
    enforceInferred: enforceInferred.value,
    showDanglingDefReferences: showDanglingDefReferences.value,
  });
}
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <h1 class="text-text text-xl font-semibold">
      {{ t("settings.heading") }}
    </h1>

    <section
      class="flex max-w-2xl flex-col gap-4"
      aria-labelledby="interface-settings-heading"
    >
      <h2
        id="interface-settings-heading"
        class="text-text text-base font-semibold"
      >
        {{ t("settings.interfaceHeading") }}
      </h2>
      <LanguagePicker />
    </section>

    <section
      class="flex max-w-2xl flex-col gap-4"
      aria-labelledby="network-settings-heading"
    >
      <h2
        id="network-settings-heading"
        class="text-text text-base font-semibold"
      >
        {{ t("settings.network.heading") }}
      </h2>

      <Message
        v-if="appSettingsRecovered"
        severity="warn"
        :closable="false"
        data-testid="settings-app-settings-recovered-warning"
      >
        {{ t("settings.network.recoveredWarning") }}
      </Message>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.network.allowNetwork.label") }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.network.allowNetwork.description") }}
          </p>
        </span>
        <ToggleSwitch
          :model-value="appSettings?.settings.network.allowNetwork ?? false"
          :disabled="appSettingsPending"
          class="shrink-0"
          data-testid="settings-allow-network"
          @update:model-value="toggleAllowNetwork"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{
            t("settings.network.autoRefreshRuleDatabases.label")
          }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.network.autoRefreshRuleDatabases.description") }}
          </p>
        </span>
        <ToggleSwitch
          :model-value="appSettings?.settings.network.autoRefreshRuleDatabases ?? false"
          :disabled="appSettingsPending"
          class="shrink-0"
          data-testid="settings-auto-refresh-rule-databases"
          @update:model-value="toggleAutoRefreshRuleDatabases"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.network.checkForUpdates.label") }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.network.checkForUpdates.description") }}
          </p>
        </span>
        <ToggleSwitch
          :model-value="appSettings?.settings.network.checkForUpdates ?? false"
          :disabled="appSettingsPending"
          class="shrink-0"
          data-testid="settings-check-for-updates"
          @update:model-value="toggleCheckForUpdates"
        />
      </label>

      <div class="flex flex-col gap-1">
        <div>
          <span
            :tabindex="appSettings?.settings.network.allowNetwork ? undefined : 0"
            :title="
              appSettings?.settings.network.allowNetwork
                ? undefined
                : t('settings.network.checkNow.disabledTitle')
            "
          >
            <Button
              :label="t('settings.network.checkNow.button')"
              severity="secondary"
              outlined
              size="small"
              :loading="checkingForUpdate"
              :disabled="!(appSettings?.settings.network.allowNetwork ?? false)"
              data-testid="settings-check-for-update-now"
              @click="checkForUpdateNow"
            />
          </span>
        </div>
        <p
          v-if="lastCheckResult"
          class="text-text-muted text-xs"
          data-testid="settings-check-for-update-result"
        >
          {{ tm(describeCheckForUpdateOutcome(lastCheckResult, locale)) }}
        </p>
        <p
          v-if="lastCheckResult && checkForUpdateTechnicalDetail(lastCheckResult)"
          class="text-text-faint font-mono text-xs break-all select-text"
          data-testid="settings-check-for-update-result-detail"
        >
          {{ t("common.technicalDetail", { detail: checkForUpdateTechnicalDetail(lastCheckResult) }) }}
        </p>
      </div>

      <div class="flex flex-col gap-1">
        <span class="text-text text-sm font-medium">{{ t("settings.network.mutedKinds.heading") }}</span>
        <p
          v-if="!mutedKinds || mutedKinds.length === 0"
          class="text-text-faint text-xs"
          data-testid="settings-muted-kinds-empty"
        >
          {{ t("settings.network.mutedKinds.empty") }}
        </p>
        <ul
          v-else
          class="flex flex-col gap-1"
          data-testid="settings-muted-kinds-list"
        >
          <li
            v-for="kind in mutedKinds"
            :key="kind"
            class="flex items-center justify-between gap-2 text-sm"
            :data-testid="`settings-muted-kind-${kind}`"
          >
            <span class="text-text-muted">{{ tm(notificationKindLabel(kind)) }}</span>
            <Button
              :label="t('settings.network.mutedKinds.unmuteButton')"
              size="small"
              text
              :loading="unmutingKind === kind"
              :data-testid="`settings-unmute-${kind}`"
              @click="unmuteKind(kind)"
            />
          </li>
        </ul>
      </div>

      <div>
        <Button
          :label="t('settings.network.restoreDefaultsButton')"
          severity="secondary"
          outlined
          size="small"
          :loading="restoringNetworkDefaults"
          aria-describedby="settings-restore-network-defaults-hint"
          data-testid="settings-restore-network-defaults"
          @click="restoreNetworkDefaultsClick"
        />
        <p
          id="settings-restore-network-defaults-hint"
          class="mt-1 text-sm text-text-muted"
        >
          {{ t("settings.network.resetIncludesSteam") }}
        </p>
      </div>
    </section>

    <div class="flex max-w-2xl flex-col gap-6">
      <label class="flex flex-col gap-1 text-sm">
        {{ t("settings.thresholdLabel") }}
        <InputNumber
          v-model="threshold"
          :min="0"
          :max="100"
          :max-fraction-digits="0"
          locale="en-US"
          data-testid="settings-threshold"
        />
      </label>

      <div class="flex flex-col gap-1 text-sm">
        <span class="text-text font-medium">{{ t("settings.tieBreak.label") }}</span>
        <p class="text-text-muted text-xs">
          {{ t("settings.tieBreak.description") }}
        </p>
        <div
          class="bg-surface-2 flex w-fit overflow-hidden rounded text-xs"
          role="radiogroup"
          :aria-label="t('settings.tieBreak.label')"
          data-testid="settings-tie-break"
          @keydown="moveTieBreak"
        >
          <button
            ref="rebuildButton"
            type="button"
            class="cursor-pointer px-3 py-1.5 font-medium transition-colors focus-visible:outline"
            :class="
              tieBreak === 'rebuild'
                ? 'bg-accent text-on-accent'
                : 'text-text-muted hover:bg-surface-1 hover:text-text'
            "
            role="radio"
            :aria-checked="tieBreak === 'rebuild'"
            :tabindex="tieBreak === 'rebuild' ? 0 : -1"
            data-testid="settings-tie-break-rebuild"
            @click="tieBreak = 'rebuild'"
          >
            {{ t("settings.tieBreak.rebuild") }}
          </button>
          <button
            ref="preserveCurrentButton"
            type="button"
            class="cursor-pointer px-3 py-1.5 font-medium transition-colors focus-visible:outline"
            :class="
              tieBreak === 'preserveCurrent'
                ? 'bg-accent text-on-accent'
                : 'text-text-muted hover:bg-surface-1 hover:text-text'
            "
            role="radio"
            :aria-checked="tieBreak === 'preserveCurrent'"
            :tabindex="tieBreak === 'preserveCurrent' ? 0 : -1"
            data-testid="settings-tie-break-preserve-current"
            @click="tieBreak = 'preserveCurrent'"
          >
            {{ t("settings.tieBreak.preserveCurrent") }}
          </button>
        </div>
      </div>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.useImportedPairs.label") }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.useImportedPairs.description") }}
          </p>
        </span>
        <ToggleSwitch
          v-model="useImportedPairs"
          class="shrink-0"
          data-testid="settings-use-imported-pairs"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.useImportedPlacements.label") }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.useImportedPlacements.description") }}
          </p>
        </span>
        <ToggleSwitch
          v-model="useImportedPlacements"
          class="shrink-0"
          data-testid="settings-use-imported-placements"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.enforceSoft.label") }}</span>
          <!-- eslint-disable vue/no-bare-strings-in-template -- a literal
               .NET type name inside `<code>`, never translated, the same
               category as a keyboard-shortcut label; every other string
               in this block already goes through `t()`. -->
          <i18n-t
            keypath="settings.enforceSoft.description"
            tag="p"
            class="text-text-muted text-xs"
          >
            <template #assemblyRef><code>AssemblyRef</code></template>
          </i18n-t>
          <!-- eslint-enable vue/no-bare-strings-in-template -->
        </span>
        <ToggleSwitch
          v-model="enforceSoft"
          class="shrink-0"
          data-testid="settings-enforce-soft"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.enforceAwareness.label") }}</span>
          <!-- eslint-disable vue/no-bare-strings-in-template -- literal
               Harmony/RimWorld API names inside `<code>`, never
               translated, the same category as a keyboard-shortcut
               label; every other string in this block already goes
               through `t()`. -->
          <i18n-t
            keypath="settings.enforceAwareness.description"
            tag="p"
            class="text-text-muted text-xs"
          >
            <template #edgeKinds><code>FindMod</code>/<code>MayRequire</code>/<code>PatchTargetsDef</code>/<code>IfModActive</code>/<code>UsesType</code>/<code>PatchSelectsInjectedNode</code></template>
          </i18n-t>
          <!-- eslint-enable vue/no-bare-strings-in-template -->
        </span>
        <ToggleSwitch
          v-model="enforceAwareness"
          class="shrink-0"
          data-testid="settings-enforce-awareness"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span class="min-w-0">
          <span class="text-text font-medium">{{ t("settings.enforceInferred.label") }}</span>
          <!-- eslint-disable vue/no-bare-strings-in-template -- literal
               analyzer engine-edge kind names inside `<code>`, never
               translated, the same category as a keyboard-shortcut
               label; every other string in this block already goes
               through `t()`. -->
          <i18n-t
            keypath="settings.enforceInferred.description"
            tag="p"
            class="text-text-muted text-xs"
          >
            <template #edgeKinds><code>PatchRemovedNode</code>/<wbr><code>RetextureAfterOwner</code>/<wbr><code>DefOverrideAfterOrigin</code>/<wbr><code>PatchInvalidatesPredicate</code>/<wbr><code>PatchRemovedNodeCosmetic</code></template>
          </i18n-t>
          <!-- eslint-enable vue/no-bare-strings-in-template -->
        </span>
        <ToggleSwitch
          v-model="enforceInferred"
          class="shrink-0"
          data-testid="settings-enforce-inferred"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.suggestMergeWhenClean.label") }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.suggestMergeWhenClean.description") }}
          </p>
        </span>
        <ToggleSwitch
          v-model="suggestMergeWhenClean"
          class="shrink-0"
          data-testid="settings-suggest-merge-when-clean"
        />
      </label>

      <label class="flex items-start justify-between gap-4 text-sm">
        <span>
          <span class="text-text font-medium">{{ t("settings.showDanglingDefReferences.label") }}</span>
          <p class="text-text-muted text-xs">
            {{ t("settings.showDanglingDefReferences.description") }}
          </p>
        </span>
        <ToggleSwitch
          v-model="showDanglingDefReferences"
          class="shrink-0"
          data-testid="settings-show-dangling-def-references"
        />
      </label>

      <div class="flex items-center gap-3">
        <Button
          :label="t('settings.saveButton')"
          :loading="isLoading"
          :disabled="threshold === null"
          data-testid="settings-save"
          @click="submit"
        />
        <Button
          :label="t('settings.resetButton')"
          severity="secondary"
          outlined
          data-testid="settings-reset-defaults-button"
          @click="resetConfirmVisible = true"
        />
        <span
          v-if="isDirty"
          class="text-text-muted text-xs"
          data-testid="settings-dirty-indicator"
        >
          {{ t("settings.dirtyIndicator") }}
        </span>
      </div>

      <AboutSection />
    </div>

    <Dialog
      v-model:visible="resetConfirmVisible"
      modal
      :header="t('settings.resetConfirm.header')"
      data-testid="settings-reset-confirm-dialog"
    >
      <p class="text-text-muted max-w-sm text-sm">
        {{ t("settings.resetConfirm.body") }}
      </p>
      <div class="mt-4 flex justify-end gap-2">
        <Button
          :label="t('settings.resetConfirm.cancelButton')"
          severity="secondary"
          data-testid="settings-reset-confirm-cancel"
          @click="resetConfirmVisible = false"
        />
        <Button
          :label="t('settings.resetConfirm.confirmButton')"
          :loading="loadingDefaults"
          data-testid="settings-reset-confirm-submit"
          @click="confirmReset"
        />
      </div>
    </Dialog>
  </div>
</template>
