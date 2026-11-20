<script setup lang="ts">
import Button from "primevue/button";
import Message from "primevue/message";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useRescanMutation } from "@/queries/activeSet";
import type { OrganizationDto } from "@/types/generated/OrganizationDto";
import { assertNever } from "@/utils/assertNever";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * Says why groups and colours cannot be used, when they cannot. A file that could not be read may
 * only be locked for a moment (another program holding it), so "Rescan and try again" comes first
 * (it is a full rescan of the mods, the only thing that reads the file again, and is worded so)
 * and "Start over" is offered only after a retry failed. A file that
 * is damaged, too large or from a newer Rimmerge offers "Start over" at once, since reading it
 * again cannot help. Start over keeps a copy of the file. Renders nothing while groups work.
 */
const { organization } = defineProps<{ organization: OrganizationDto | undefined }>();
const emit = defineEmits<{ startOver: [] }>();

const { t } = useI18n();
const tm = useTranslateMessage();
const { mutateAsync: rescan, isLoading: isRetrying } = useRescanMutation();
const hasRetried = ref(false);
const retryError = ref<CommandErrorDescriptor | null>(null);

async function tryAgain(): Promise<void> {
  retryError.value = null;
  try {
    await rescan();
  } catch (caught: unknown) {
    // The rescan failed outright (not merely "still unreadable"): say so here, next to the button.
    retryError.value = describeCommandError(caught);
  } finally {
    hasRetried.value = true;
  }
}

/** The sentence for this state, or `null` while groups work. */
const text = computed<string | null>(() => {
  if (organization === undefined || organization.kind === "loaded") {
    return null;
  }
  if (organization.kind === "notLoaded") {
    return t("mods.groups.notice.notLoaded");
  }
  const issue = organization.issue;
  switch (issue.kind) {
    case "unreadable":
      return t("mods.groups.notice.unreadable");
    case "tooLarge":
      return t("mods.groups.notice.tooLarge");
    case "malformed":
      return t("mods.groups.notice.malformed");
    case "newerVersion":
      return t("mods.groups.notice.newerVersion");
    case "invalid":
      return t("mods.groups.notice.invalid");
    default:
      return assertNever(issue);
  }
});

const isRecovered = computed(() => organization?.kind === "recovered");
const isUnreadable = computed(
  () => organization?.kind === "recovered" && organization.issue.kind === "unreadable",
);
const canTryAgain = computed(() => isUnreadable.value);
const canStartOver = computed(() => isRecovered.value && (!isUnreadable.value || hasRetried.value));
</script>

<template>
  <Message
    v-if="text !== null"
    severity="warn"
    :closable="false"
    data-testid="organization-notice"
  >
    <div class="flex flex-wrap items-center gap-3">
      <span>{{ text }}</span>
      <Button
        v-if="canTryAgain"
        :label="t('mods.groups.notice.tryAgain')"
        size="small"
        severity="secondary"
        :loading="isRetrying"
        data-testid="organization-try-again"
        @click="tryAgain"
      />
      <Button
        v-if="canStartOver"
        :label="t('mods.groups.notice.startOver')"
        size="small"
        severity="danger"
        outlined
        data-testid="organization-start-over"
        @click="emit('startOver')"
      />
    </div>
    <div
      v-if="retryError"
      class="mt-2 text-sm"
      role="alert"
      data-testid="organization-retry-error"
    >
      <div class="font-medium">
        {{ tm(retryError.title) }}
      </div>
      <div>{{ tm(retryError.detail) }}</div>
    </div>
  </Message>
</template>
