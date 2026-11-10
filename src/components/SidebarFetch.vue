<script setup lang="ts">
import { computed } from "vue";
import { storeToRefs } from "pinia";
import type { DropdownMenuItem } from "@nuxt/ui";
import { useToast } from "@nuxt/ui/composables";
import { useTimeAgo } from "@vueuse/core";
import { useJiraIssuesStore } from "@/features/jira";
import { usePullRequestsStore } from "@/features/github";
import { useWorkstreamsStore } from "@/features/workstreams";
import type { GitHubSyncSummary, JiraSyncSummary } from "@/types";

/** Fetch buttons for Jira and GitHub, available on every page from the sidebar. */
defineProps<{ collapsed?: boolean }>();

const jiraStore = useJiraIssuesStore();
const pullRequestsStore = usePullRequestsStore();
const workstreamsStore = useWorkstreamsStore();
const toast = useToast();

const { lastSyncedAt: jiraSyncedAt, syncing: jiraSyncing } = storeToRefs(jiraStore);
const { lastSyncedAt: githubSyncedAt, syncing: githubSyncing } = storeToRefs(pullRequestsStore);
const jiraAgo = useTimeAgo(computed(() => jiraSyncedAt.value ?? 0));
const githubAgo = useTimeAgo(computed(() => githubSyncedAt.value ?? 0));
const syncingAny = computed(() => jiraSyncing.value || githubSyncing.value);

/** Workstreams show items of both sources, so the open one is reloaded after a fetch. */
async function reloadWorkstreams(): Promise<void> {
  const openId = workstreamsStore.current?.workstream.id;
  await Promise.all([
    workstreamsStore.load(),
    openId === undefined ? undefined : workstreamsStore.loadDetail(openId),
  ]);
}

function failed(title: string, message: string | undefined): void {
  toast.add({ title, description: message, icon: "i-lucide-circle-alert", color: "error" });
}

async function fetchJira(full = false): Promise<void> {
  const summary = await jiraStore.sync(full);
  if (!summary) return failed("Jira fetch failed", jiraStore.error?.message);
  toast.add({ title: describeJira(summary), icon: "i-lucide-ticket", color: "success" });
  if (summary.pullRequestError) {
    toast.add({
      title: "Related pull requests not updated",
      description: summary.pullRequestError,
      icon: "i-lucide-triangle-alert",
      color: "warning",
    });
  }
  // A Jira fetch also updates the pull requests related to the issues.
  await Promise.all([pullRequestsStore.load(), reloadWorkstreams()]);
}

async function fetchPullRequests(): Promise<void> {
  const summary = await pullRequestsStore.sync();
  if (!summary) return failed("Pull request fetch failed", pullRequestsStore.error?.message);
  toast.add({ title: describeGitHub(summary), icon: "i-lucide-git-pull-request", color: "success" });
  await reloadWorkstreams();
}

function fetchAll(): Promise<unknown> {
  return Promise.all([fetchJira(), fetchPullRequests()]);
}

function describeJira(summary: JiraSyncSummary): string {
  if (summary.mode === "full") return `Fetched ${summary.upserted} Jira issues`;
  if (summary.upserted === 0 && summary.removed === 0) return "Jira issues up to date";
  return `Jira: ${summary.upserted} updated, ${summary.removed} removed`;
}

function describeGitHub(summary: GitHubSyncSummary): string {
  const parts = [summary.fetched === 1 ? "1 pull request" : `${summary.fetched} pull requests`];
  if (summary.removed) parts.push(`${summary.removed} no longer need you`);
  if (summary.others) parts.push(`${summary.others} other open`);
  return parts.join(", ");
}

const jiraMenu: DropdownMenuItem[] = [
  {
    label: "Full refresh",
    description: "Re-fetch everything, e.g. to drop issues deleted in Jira",
    icon: "i-lucide-database-zap",
    onSelect: () => void fetchJira(true),
  },
];

const sources = computed(() => [
  {
    key: "jira",
    label: "Jira issues",
    icon: "i-lucide-ticket",
    syncedAt: jiraSyncedAt.value,
    ago: jiraAgo.value,
    syncing: jiraSyncing.value,
    fetch: () => fetchJira(),
    menu: jiraMenu,
  },
  {
    key: "github",
    label: "Pull requests",
    icon: "i-lucide-git-pull-request",
    syncedAt: githubSyncedAt.value,
    ago: githubAgo.value,
    syncing: githubSyncing.value,
    fetch: fetchPullRequests,
    menu: null,
  },
]);
</script>

<template>
  <UTooltip v-if="collapsed" text="Fetch Jira issues and pull requests" :content="{ side: 'right' }">
    <UButton
      color="neutral"
      variant="ghost"
      icon="i-lucide-refresh-cw"
      aria-label="Fetch all"
      :loading="syncingAny"
      @click="fetchAll"
    />
  </UTooltip>

  <div v-else class="flex w-full flex-col gap-1">
    <UButton
      color="neutral"
      variant="subtle"
      icon="i-lucide-refresh-cw"
      label="Fetch all"
      block
      :loading="syncingAny"
      @click="fetchAll"
    />
    <div v-for="source in sources" :key="source.key" class="flex items-center gap-1 pl-2.5">
      <UIcon :name="source.icon" class="size-4 shrink-0 text-muted" />
      <div class="min-w-0 flex-1 px-1">
        <p class="truncate text-sm text-default">{{ source.label }}</p>
        <p class="truncate text-xs text-muted">
          {{ source.syncedAt ? `Fetched ${source.ago}` : "Not fetched yet" }}
        </p>
      </div>
      <UDropdownMenu v-if="source.menu" :items="source.menu" :content="{ align: 'end' }">
        <UButton
          color="neutral"
          variant="ghost"
          size="sm"
          icon="i-lucide-ellipsis"
          :aria-label="`More ${source.label} options`"
          :disabled="source.syncing"
        />
      </UDropdownMenu>
      <UTooltip :text="`Fetch ${source.label.toLowerCase()}`">
        <UButton
          color="neutral"
          variant="ghost"
          size="sm"
          icon="i-lucide-refresh-cw"
          :aria-label="`Fetch ${source.label.toLowerCase()}`"
          :loading="source.syncing"
          @click="source.fetch()"
        />
      </UTooltip>
    </div>
  </div>
</template>
