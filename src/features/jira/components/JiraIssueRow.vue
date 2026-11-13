<script setup lang="ts">
import { computed } from "vue";
import { storeToRefs } from "pinia";
import { statusRefId, useJiraIssuesStore } from "../stores/jiraIssuesStore";
import type { JiraIssue } from "@/types";

const props = defineProps<{ issue: JiraIssue }>();

const store = useJiraIssuesStore();
const { statusOptions } = storeToRefs(store);

/** Badge color per Jira status category (To Do / In Progress / Done). */
const categoryColors = {
  new: "neutral",
  indeterminate: "info",
  done: "success",
} as const;

/** Epics purple, bugs red, every other work type blue. */
const issueTypeColor = computed(() => {
  switch (props.issue.issueType.toLowerCase()) {
    case "epic":
      return "epic";
    case "bug":
      return "error";
    default:
      return "info";
  }
});

const statusColor = computed(() => {
  const id = statusRefId(props.issue);
  const category = statusOptions.value.find((status) => statusRefId(status) === id)?.category;
  return categoryColors[category as keyof typeof categoryColors] ?? "neutral";
});
</script>

<template>
  <li class="flex items-center gap-3 px-4 py-2.5">
    <UBadge color="neutral" variant="subtle" class="shrink-0 font-mono" :label="issue.key" />
    <UBadge
      v-if="issue.issueType"
      :color="issueTypeColor"
      variant="subtle"
      class="shrink-0"
      :label="issue.issueType"
    />
    <ULink
      as="button"
      class="min-w-0 flex-1 truncate text-left"
      :title="`Open ${issue.key} in Jira`"
      @click="store.open(issue.key)"
    >
      {{ issue.summary }}
    </ULink>
    <!-- Related items from other features, e.g. pull requests mentioning the issue. -->
    <slot name="related" />
    <UTooltip
      v-if="!issue.assignedToMe && issue.watched"
      text="Not assigned to you: listed because its status is watched (Jira settings)"
    >
      <UIcon name="i-lucide-users" class="size-4 shrink-0 text-muted" />
    </UTooltip>
    <UTooltip v-else-if="!issue.assignedToMe" text="No longer assigned to you">
      <UIcon name="i-lucide-user-x" class="size-4 shrink-0 text-muted" />
    </UTooltip>
    <UBadge
      v-if="issue.statusName"
      :color="statusColor"
      variant="subtle"
      class="shrink-0"
      :label="issue.statusName"
    />
    <slot name="actions" />
  </li>
</template>
