<script setup lang="ts">
import { useQuery } from "@pinia/colada";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import { useToast } from "primevue/usetoast";
import { computed, onMounted, ref, watchEffect } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import BaseProgressBar from "@/components/base/BaseProgressBar.vue";
import LanguagePicker from "@/components/settings/LanguagePicker.vue";
import { useTauriEvent } from "@/composables/useTauriEvent";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { MessageDescriptor } from "@/i18n/messageDescriptor";
import { queryKeys } from "@/queries/keys";
import { useRunLaunchNetworkChecksMutation } from "@/queries/notifications";
import { pickFolder } from "@/services/dialogs";
import { getDefaultPaths, loadProject, saveAppConfig } from "@/services/ipc";
import { useSessionStore } from "@/stores/session";
import type { ProgressEventDto } from "@/types/generated/ProgressEventDto";
import { describeDefaultPathsError, describeDefaultPathsWarning } from "@/utils/defaultPaths";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { ruleWarningsDetail } from "@/utils/ruleWarning";
import { scanStageLabel } from "@/utils/scanStage";

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const route = useRoute();
const router = useRouter();
const session = useSessionStore();
const toast = useToast();

const { data: defaults } = useQuery({
  key: queryKeys.defaultPaths,
  query: getDefaultPaths,
});
const { mutate: runLaunchNetworkChecks } = useRunLaunchNetworkChecksMutation();

const gameDir = ref("");
const workshopDir = ref("");
const modsConfig = ref("");
const profileDir = ref("");

watchEffect(() => {
  if (!defaults.value) {
    return;
  }
  if (!gameDir.value && defaults.value.gameDir) {
    gameDir.value = defaults.value.gameDir;
  }
  if (!workshopDir.value && defaults.value.workshopDir) {
    workshopDir.value = defaults.value.workshopDir;
  }
  if (!modsConfig.value && defaults.value.modsConfig) {
    modsConfig.value = defaults.value.modsConfig;
  }
  if (!profileDir.value && defaults.value.profileDir) {
    profileDir.value = defaults.value.profileDir;
  }
});

/**
 * The backend's own "no RimWorld install found" text, which names every
 * candidate location it looked at, one per line. Shown as a persistent
 * message rather than a toast: on a machine with no detectable install
 * this is the whole reason the four fields are empty, and the user needs
 * to be able to read the list while typing a path into the first one.
 * Kept alongside {@link detectionErrorDescriptor} as the "technical
 * details" line — it names real file paths, which are never translated.
 */
const detectionError = computed(() => defaults.value?.error ?? null);

/** The localized counterpart of {@link detectionError}, from `get_default_paths`'s structured `errorCode` — `null` for an older/mocked response that doesn't carry one, in which case {@link detectionError} alone still renders. */
const detectionErrorDescriptor = computed<MessageDescriptor | null>(() =>
  defaults.value?.errorCode ? describeDefaultPathsError(defaults.value.errorCode) : null,
);

/**
 * A resolved-but-suspicious install: an explicit `RIMMERGE_GAME_DIR` or a
 * pinned `config.json` entry naming a directory with no `Version.txt` and
 * `Data/Core/`. Shown alongside the (still prefilled) fields rather than
 * blocking Load — the drive may simply not be mounted yet, and the scan
 * that follows will say far more about why it failed than a refusal here
 * could. Same sentence `rimmerge config set` prints.
 *
 * Only ever reflects what the *backend* resolved on load, so it stops
 * being shown the moment the user edits the field: keeping it live would
 * need a per-keystroke round trip for a hint the Load button is about to
 * settle anyway.
 */
const resolvedWarning = computed(() => {
  const warning = defaults.value?.warning ?? null;
  if (warning === null || gameDir.value !== defaults.value?.gameDir) {
    return null;
  }
  return warning;
});

/** The localized counterpart of {@link resolvedWarning} — see {@link detectionErrorDescriptor} for the fallback behavior. */
const resolvedWarningDescriptor = computed<MessageDescriptor | null>(() =>
  resolvedWarning.value !== null && defaults.value?.warningCode
    ? describeDefaultPathsWarning(defaults.value.warningCode)
    : null,
);

async function browseFolder(target: "gameDir" | "workshopDir" | "profileDir"): Promise<void> {
  const targets = { gameDir, workshopDir, profileDir };
  const field = targets[target];
  const picked = await pickFolder(field.value || undefined);
  if (picked !== null) {
    field.value = picked;
  }
}

const progress = ref<ProgressEventDto | null>(null);
const loading = ref(false);
const errorMessage = ref<CommandErrorDescriptor | null>(null);

useTauriEvent<ProgressEventDto>("project://progress", (payload) => {
  progress.value = payload;
});

async function submit(): Promise<void> {
  errorMessage.value = null;
  progress.value = null;
  loading.value = true;
  try {
    const paths = {
      gameDir: gameDir.value,
      workshopDir: workshopDir.value,
      modsConfig: modsConfig.value,
      profileDir: profileDir.value,
    };
    const summary = await loadProject(paths);
    // Scan notes (`summary.warnings`) can run to dozens on a large
    // install — they're not actionable, just informational, and shown on
    // the dashboard's "Scan notes" card and each named mod's own page
    // instead of as toasts (a wall of stacked toasts the user can't
    // scroll was the previous behaviour). `submit` navigates straight to
    // the dashboard on success, so the count still needs *some*
    // acknowledgement here or it'd go unnoticed — one small toast, not one
    // per note.
    session.setScanNotes(summary.warnings);
    if (summary.warnings.length > 0) {
      toast.add({
        severity: "info",
        summary: t(
          "setup.scanFinishedToast",
          { count: summary.warnings.length },
          summary.warnings.length,
        ),
        life: 6_000,
      });
    }
    // Rules-file load warnings (a `rules.json` migration notice, or a
    // mod-knowledge data note; see `RulesLoadWarning`) are actionable and few, so they keep their own
    // toast rather than folding into the scan-notes count above. A toast
    // survives the navigation (`<Toast />` lives in `App.vue`, above the
    // router view), unlike anything scoped to this page's own template.
    // Each warning is a code the frontend renders (`describeRuleWarning`).
    if (summary.ruleWarnings.length > 0) {
      toast.add({
        severity: "warn",
        summary: t("setup.projectLoadedWithWarningsToast"),
        detail: ruleWarningsDetail(summary.ruleWarnings, t, locale.value),
        life: 10_000,
      });
    }
    session.setLoaded(paths, summary.selected);
    // Fire-and-forget: the backend decides everything (the first-run
    // gate, the once-per-launch flag, `allowNetwork`/`checkForUpdates`),
    // and this call never blocks navigation. The mutation's own
    // `onSuccess` refetches the notice list once it resolves, so a new
    // `Welcome`/`UpdateAvailable`/stale-database notice appears in the
    // bell without the user doing anything.
    runLaunchNetworkChecks();
    await router.push({ name: "dashboard" });
  } catch (error: unknown) {
    errorMessage.value = describeCommandError(error);
  } finally {
    loading.value = false;
  }
}

const saving = ref(false);

/**
 * Pins the three install paths into `<base>/config.json` so the next
 * launch prefills them without detection — the desktop half of
 * `rimmerge config set`.
 *
 * `profileDir` is deliberately not saved: it is *derived* from the
 * `ModsConfig.xml` path (`rim_io::profile_dir` hashes it), so pinning it
 * alongside a `mods_config` it might not match is how two installs end up
 * sharing one profile's decisions.
 */
async function saveDefaults(): Promise<void> {
  errorMessage.value = null;
  saving.value = true;
  try {
    await saveAppConfig({
      gameDir: gameDir.value,
      workshopDir: workshopDir.value,
      modsConfig: modsConfig.value,
    });
    toast.add({
      severity: "success",
      summary: t("setup.savedAsDefaultsToast"),
      detail: t("setup.savedAsDefaultsDetail"),
      life: 6_000,
    });
  } catch (error: unknown) {
    errorMessage.value = describeCommandError(error);
  } finally {
    saving.value = false;
  }
}

// Transparent reload (`main.ts`'s own
// session-lost handler doc comment has the full rationale): landing here
// with `?autoReload=1` means a caught backend panic discarded the
// session mid-use, and `main.ts` already kept the paths it was loaded
// from (`session.paths`) rather than clearing them. Prefilling from those
// and calling `submit()` immediately re-runs the exact same load flow —
// same progress bar, same error handling on a genuine second failure —
// a manual "Load project" click would, just without the user having to
// click it. Runs once, on mount, never re-triggered by a later query
// change on this same page instance. A missing `session.paths` (the
// query flag present with nothing to reload — not expected in practice,
// `main.ts` only sets it when paths are known) silently falls through to
// the ordinary blank/defaults-prefilled form instead of guessing.
onMounted(() => {
  if (route.query["autoReload"] !== "1" || session.paths === null) {
    return;
  }
  gameDir.value = session.paths.gameDir;
  workshopDir.value = session.paths.workshopDir;
  modsConfig.value = session.paths.modsConfig;
  profileDir.value = session.paths.profileDir;
  void submit();
});
</script>

<template>
  <main class="mx-auto flex max-w-2xl flex-col gap-4 p-8">
    <div class="flex items-start justify-between gap-4">
      <h1 class="text-text text-xl font-semibold">
        {{ t("setup.title") }}
      </h1>
      <LanguagePicker class="w-56 shrink-0" />
    </div>

    <Message
      v-if="detectionError"
      severity="warn"
      data-testid="setup-detection-error"
    >
      <div
        v-if="detectionErrorDescriptor"
        class="font-medium"
      >
        {{ tm(detectionErrorDescriptor) }}
      </div>
      <span class="whitespace-pre-wrap">{{ detectionError }}</span>
    </Message>

    <Message
      v-if="resolvedWarning"
      severity="warn"
      data-testid="setup-path-warning"
    >
      <div
        v-if="resolvedWarningDescriptor"
        class="font-medium"
      >
        {{ tm(resolvedWarningDescriptor) }}
      </div>
      {{ resolvedWarning }}
    </Message>

    <label class="flex flex-col gap-1">
      <span class="text-sm font-medium">{{ t("setup.fields.gameDir") }}</span>
      <div class="flex gap-2">
        <InputText
          v-model="gameDir"
          class="flex-1"
          data-testid="game-dir-input"
        />
        <Button
          :label="t('setup.browse')"
          severity="secondary"
          @click="browseFolder('gameDir')"
        />
      </div>
    </label>

    <label class="flex flex-col gap-1">
      <span class="text-sm font-medium">{{ t("setup.fields.workshopDir") }}</span>
      <div class="flex gap-2">
        <InputText
          v-model="workshopDir"
          class="flex-1"
          data-testid="workshop-dir-input"
        />
        <Button
          :label="t('setup.browse')"
          severity="secondary"
          @click="browseFolder('workshopDir')"
        />
      </div>
    </label>

    <label class="flex flex-col gap-1">
      <span class="text-sm font-medium">{{ t("setup.fields.modsConfig") }}</span>
      <InputText
        v-model="modsConfig"
        data-testid="mods-config-input"
      />
    </label>

    <label class="flex flex-col gap-1">
      <span class="text-sm font-medium">{{ t("setup.fields.profileDir") }}</span>
      <div class="flex gap-2">
        <InputText
          v-model="profileDir"
          class="flex-1"
          data-testid="profile-dir-input"
        />
        <Button
          :label="t('setup.browse')"
          severity="secondary"
          @click="browseFolder('profileDir')"
        />
      </div>
    </label>

    <Message
      v-if="errorMessage"
      severity="error"
      data-testid="setup-error"
    >
      <div class="font-medium">
        {{ tm(errorMessage.title) }}
      </div>
      <div>{{ tm(errorMessage.detail) }}</div>
      <div
        v-if="errorMessage.technicalDetail"
        class="text-xs opacity-75"
      >
        {{ errorMessage.technicalDetail }}
      </div>
    </Message>

    <BaseProgressBar
      v-if="loading"
      :progress="{ done: progress?.done ?? 0, total: progress?.total ?? 0 }"
      :label="t('setup.loadingProject')"
      :caption="progress ? tm(scanStageLabel(progress.stage)) : t('setup.startingCaption')"
      data-testid="setup-progress"
    />

    <div class="flex gap-2">
      <Button
        :label="t('setup.loadProject')"
        class="flex-1"
        :loading="loading"
        :disabled="!gameDir || !workshopDir || !modsConfig || !profileDir"
        data-testid="load-project-button"
        @click="submit"
      />
      <Button
        :label="t('setup.saveAsDefaults')"
        severity="secondary"
        :loading="saving"
        :disabled="!gameDir && !workshopDir && !modsConfig"
        data-testid="save-defaults-button"
        @click="saveDefaults"
      />
    </div>
  </main>
</template>
