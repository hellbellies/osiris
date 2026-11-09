<script setup lang="ts">
import { computed, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useToast } from "@nuxt/ui/composables";
import { useTimeAgo } from "@vueuse/core";
import { useJiraIssuesStore } from "../stores/jiraIssuesStore";
import JiraIssueRow from "./JiraIssueRow.vue";
import JiraStatusFilter from "./JiraStatusFilter.vue";
import type { JiraIssue, JiraSyncSummary } from "@/types";

defineSlots<{
  /** Related items shown after each row's summary. */
  related?(props: { issue: JiraIssue }): unknown;
  /** Extra controls at the end of each row. */
  actions?(props: { issue: JiraIssue }): unknown;
}>();

const store = useJiraIssuesStore();
const {
  issues,
  visibleIssues,
  selectedStatuses,
  lastSyncedAt,
  loaded,
  syncing,
  error,
} = storeToRefs(store);
const toast = useToast();

const lastSyncedAgo = useTimeAgo(computed(() => lastSyncedAt.value ?? 0));

/** Visible issues grouped by project (space), alphabetically; issue order is kept. */
const projects = computed(() => {
  const groups = new Map<string, JiraIssue[]>();
  for (const issue of visibleIssues.value) {
    const group = groups.get(issue.projectKey);
    if (group) group.push(issue);
    else groups.set(issue.projectKey, [issue]);
  }
  return [...groups]
    .map(([key, items]) => ({ key, items }))
    .sort((a, b) => a.key.localeCompare(b.key));
});

async function sync(full: boolean): Promise<void> {
  const summary = await store.sync(full);
  if (summary) toast.add({ title: describe(summary), icon: "i-lucide-refresh-cw", color: "success" });
}

function describe(summary: JiraSyncSummary): string {
  if (summary.mode === "full") return `Fetched ${summary.upserted} issues`;
  if (summary.upserted === 0 && summary.removed === 0) return "Already up to date";
  return `${summary.upserted} updated, ${summary.removed} removed`;
}

// Local data only, so always reload: other views may have changed workstream links.
onMounted(() => void store.load());
</script>

<template>
  <UCard :ui="{ body: 'p-0 sm:p-0' }">
    <template #header>
      <div class="flex items-center justify-between gap-4">
        <div>
          <h2 class="font-semibold text-highlighted">Assigned to me</h2>
          <p class="text-sm text-muted">
            <template v-if="lastSyncedAt">Last fetched {{ lastSyncedAgo }}</template>
            <template v-else>Not fetched yet</template>
            <template v-if="selectedStatuses.length">
              · showing {{ visibleIssues.length }} of {{ issues.length }}
            </template>
          </p>
        </div>
        <div class="flex gap-2">
          <JiraStatusFilter />
          <UTooltip text="Re-fetch everything, e.g. to drop issues deleted in Jira">
            <UButton
              color="neutral"
              variant="ghost"
              icon="i-lucide-database-zap"
              label="Full refresh"
              :disabled="syncing"
              @click="sync(true)"
            />
          </UTooltip>
          <UButton
            icon="i-lucide-refresh-cw"
            label="Fetch"
            :loading="syncing"
            @click="sync(false)"
          />
        </div>
      </div>
    </template>

    <UAlert
      v-if="error"
      class="m-4"
      color="error"
      variant="subtle"
      icon="i-lucide-circle-alert"
      :title="error.message"
    />

    <div v-if="visibleIssues.length" class="divide-y divide-default">
      <section v-for="project in projects" :key="project.key">
        <h3 class="flex items-center gap-2 bg-elevated/50 px-4 py-1.5 text-sm font-medium text-highlighted">
          <UIcon name="i-lucide-folder-kanban" class="size-4 text-muted" />
          {{ project.key || "No space" }}
          <UBadge color="neutral" variant="subtle" size="sm" :label="project.items.length" />
        </h3>
        <ul class="divide-y divide-default border-t border-default">
          <JiraIssueRow v-for="issue in project.items" :key="issue.id" :issue="issue">
            <template #related>
              <slot name="related" :issue="issue" />
            </template>
            <template #actions>
              <slot name="actions" :issue="issue" />
            </template>
          </JiraIssueRow>
        </ul>
      </section>
    </div>

    <UEmpty
      v-else-if="issues.length"
      class="py-10"
      icon="i-lucide-list-filter"
      title="No matching issues"
      description="None of your issues has one of the selected statuses."
      :actions="[
        { label: 'Show all', color: 'neutral', variant: 'subtle', onClick: () => store.setStatusFilter([]) },
      ]"
    />
    <UEmpty
      v-else-if="loaded && !syncing"
      class="py-10"
      icon="i-lucide-inbox"
      title="No issues"
      :description="
        lastSyncedAt
          ? 'Nothing in your projects is assigned to you.'
          : 'Add project keys in the settings, then fetch your issues.'
      "
    />
  </UCard>
</template>
