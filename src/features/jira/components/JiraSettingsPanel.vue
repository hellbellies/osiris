<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { useJiraStore } from "../stores/jiraStore";
import { statusRefId, useJiraIssuesStore } from "../stores/jiraIssuesStore";
import type { JiraSettingsInput, JiraStatusRef } from "@/types";

const store = useJiraStore();
const { settings, loading, saving, error, testing, testedUser, testError } = storeToRefs(store);
// The statuses each project can have, known from the last fetch.
const issuesStore = useJiraIssuesStore();
const { statusOptions } = storeToRefs(issuesStore);

const form = reactive({
  baseUrl: "",
  email: "",
  projectKeys: "",
  apiToken: "",
  watchedStatuses: [] as JiraStatusRef[],
});

/** The (project, status) being added to the watched statuses. */
const newProject = ref<string | undefined>();
const newStatus = ref<string | undefined>();

const projectItems = computed(() => settings.value?.projectKeys ?? []);
const statusItems = computed(() => {
  const taken = new Set(form.watchedStatuses.map(statusRefId));
  return statusOptions.value
    .filter((s) => s.projectKey === newProject.value && !taken.has(statusRefId(s)))
    .map((s) => ({ label: s.name, value: s.statusId }));
});

function statusName(status: JiraStatusRef): string {
  const option = statusOptions.value.find((s) => statusRefId(s) === statusRefId(status));
  return option?.name ?? `Status ${status.statusId}`;
}

function addWatched(): void {
  if (!newProject.value || !newStatus.value) return;
  form.watchedStatuses.push({ projectKey: newProject.value, statusId: newStatus.value });
  newStatus.value = undefined;
}

function removeWatched(status: JiraStatusRef): void {
  form.watchedStatuses = form.watchedStatuses.filter((s) => statusRefId(s) !== statusRefId(status));
}

watch(newProject, () => (newStatus.value = undefined));

// Sync the form from what Rust stored (e.g. the normalized URL).
watch(
  settings,
  (value) => {
    if (!value) return;
    form.baseUrl = value.baseUrl;
    form.email = value.email;
    form.projectKeys = value.projectKeys.join(", ");
    form.watchedStatuses = [...value.watchedStatuses];
    newProject.value ??= value.projectKeys[0];
  },
  { immediate: true },
);

// A stale test result would be misleading once the inputs change.
watch(form, () => store.resetTest());

function toInput(): JiraSettingsInput {
  return {
    baseUrl: form.baseUrl,
    email: form.email,
    projectKeys: form.projectKeys.split(","),
    watchedStatuses: form.watchedStatuses,
    // A blank field means "use the token already in the keychain".
    apiToken: form.apiToken.trim() ? form.apiToken : null,
  };
}

async function onSubmit(): Promise<void> {
  const saved = await store.save(toInput());
  if (saved) form.apiToken = "";
}

onMounted(() => {
  if (!settings.value) void store.load();
  if (!issuesStore.loaded) void issuesStore.load();
});
</script>

<template>
  <UCard>
    <template #header>
      <h2 class="font-semibold text-highlighted">Jira</h2>
      <p class="text-sm text-muted">The API token is stored in the system keychain.</p>
    </template>

    <div v-if="loading" class="space-y-4">
      <USkeleton class="h-8 w-full" />
      <USkeleton class="h-8 w-full" />
      <USkeleton class="h-8 w-full" />
    </div>

    <form v-else-if="settings" class="space-y-4" @submit.prevent="onSubmit">
      <UFormField label="Site URL" name="baseUrl">
        <UInput
          v-model="form.baseUrl"
          type="url"
          placeholder="https://your-team.atlassian.net"
          icon="i-lucide-link"
          class="w-full"
        />
      </UFormField>

      <UFormField label="Email" name="email" description="The Atlassian account the token belongs to.">
        <UInput
          v-model="form.email"
          type="email"
          placeholder="you@example.com"
          icon="i-lucide-at-sign"
          class="w-full"
        />
      </UFormField>

      <UFormField
        label="Projects"
        name="projectKeys"
        description="Comma-separated project keys, e.g. the FOO in FOO-123."
      >
        <UInput
          v-model="form.projectKeys"
          placeholder="FOO, BAR"
          icon="i-lucide-folder-kanban"
          class="w-full"
        />
      </UFormField>

      <UFormField
        label="Also show"
        name="watchedStatuses"
        description="All issues in these statuses are listed, whoever they are assigned to. Saved with the settings; the next fetch picks them up."
      >
        <div class="flex flex-col gap-2">
          <div v-if="form.watchedStatuses.length" class="flex flex-wrap gap-1.5">
            <UButton
              v-for="status in form.watchedStatuses"
              :key="`${status.projectKey}:${status.statusId}`"
              color="neutral"
              variant="subtle"
              size="xs"
              trailing-icon="i-lucide-x"
              :label="`${status.projectKey} · ${statusName(status)}`"
              :aria-label="`Stop showing ${status.projectKey} ${statusName(status)}`"
              @click="removeWatched(status)"
            />
          </div>
          <div class="flex gap-2">
            <USelect
              v-model="newProject"
              :items="projectItems"
              placeholder="Space"
              class="w-28"
              :disabled="!projectItems.length"
            />
            <USelect
              v-model="newStatus"
              :items="statusItems"
              placeholder="Status"
              class="min-w-0 flex-1"
              :disabled="!statusItems.length"
            />
            <UButton
              color="neutral"
              variant="outline"
              icon="i-lucide-plus"
              label="Add"
              :disabled="!newProject || !newStatus"
              @click="addWatched"
            />
          </div>
          <p v-if="projectItems.length && !statusOptions.length" class="text-xs text-muted">
            Fetch Jira issues once to load the statuses of your projects.
          </p>
        </div>
      </UFormField>

      <UFormField label="API token" name="apiToken">
        <template #description>
          <span v-if="settings?.hasToken" class="inline-flex items-center gap-1">
            <UIcon name="i-lucide-shield-check" class="text-success" />
            A token is saved. Leave blank to keep it.
          </span>
          <span v-else>No token saved yet.</span>
        </template>
        <UInput
          v-model="form.apiToken"
          type="password"
          autocomplete="off"
          :placeholder="settings?.hasToken ? '••••••••••••' : 'Paste your API token'"
          icon="i-lucide-key-round"
          class="w-full"
        />
      </UFormField>

      <UAlert
        v-if="testedUser"
        color="success"
        variant="subtle"
        icon="i-lucide-circle-check"
        :title="`Connected as ${testedUser.displayName}`"
        :description="testedUser.emailAddress ?? undefined"
      />
      <UAlert
        v-else-if="testError"
        color="error"
        variant="subtle"
        icon="i-lucide-plug-zap"
        title="Connection failed"
        :description="testError.message"
      />

      <div class="flex justify-end gap-2">
        <UButton
          v-if="settings?.hasToken"
          color="error"
          variant="ghost"
          icon="i-lucide-trash-2"
          label="Remove token"
          @click="store.clearToken()"
        />
        <UButton
          color="neutral"
          variant="outline"
          icon="i-lucide-plug"
          label="Test connection"
          :loading="testing"
          @click="store.testConnection(toInput())"
        />
        <UButton type="submit" icon="i-lucide-save" label="Save" :loading="saving" />
      </div>
    </form>

    <UAlert
      v-if="error"
      class="mt-4"
      color="error"
      variant="subtle"
      icon="i-lucide-circle-alert"
      :title="error.message"
    />
  </UCard>
</template>
