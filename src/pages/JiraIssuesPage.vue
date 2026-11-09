<script setup lang="ts">
import { JiraIssuePullRequests, usePullRequestsStore } from "@/features/github";
import { JiraIssueList, useJiraIssuesStore } from "@/features/jira";
import { WorkstreamSelect } from "@/features/workstreams";

const jiraStore = useJiraIssuesStore();
const pullRequestsStore = usePullRequestsStore();
</script>

<template>
  <UDashboardPanel id="jira">
    <template #header>
      <UDashboardNavbar title="Jira issues" icon="i-lucide-ticket" />
    </template>
    <template #body>
      <!-- A Jira fetch also updates the pull requests related to the issues. -->
      <JiraIssueList @synced="pullRequestsStore.load()">
        <template #related="{ issue }">
          <JiraIssuePullRequests :issue-id="issue.id" />
        </template>
        <template #actions="{ issue }">
          <WorkstreamSelect
            :item="{ itemType: 'jiraIssue', itemId: issue.id }"
            :workstream-id="issue.workstreamId"
            @changed="jiraStore.applyWorkstream(issue.id, $event)"
          />
        </template>
      </JiraIssueList>
    </template>
  </UDashboardPanel>
</template>
