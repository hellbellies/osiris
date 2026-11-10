<script setup lang="ts">
import { JiraIssuePullRequests } from "@/features/github";
import { JiraIssueList, useJiraIssuesStore } from "@/features/jira";
import { WorkstreamSelect } from "@/features/workstreams";

const jiraStore = useJiraIssuesStore();
</script>

<template>
  <UDashboardPanel id="jira">
    <template #header>
      <UDashboardNavbar title="Jira issues" icon="i-lucide-ticket" />
    </template>
    <template #body>
      <JiraIssueList>
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
