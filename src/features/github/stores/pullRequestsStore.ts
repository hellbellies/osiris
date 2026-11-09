import { defineStore } from "pinia";
import { ref } from "vue";
import { call } from "@/lib/ipc";
import type { AppError, GitHubSyncSummary, PullRequest } from "@/types";

/** Cached pull requests: the listed ones, and those mentioning Jira issues. */
export const usePullRequestsStore = defineStore("pullRequests", () => {
  const pullRequests = ref<PullRequest[]>([]);
  /** Pull requests (any state) mentioning each Jira issue, keyed by Jira issue id. */
  const byJiraIssue = ref<Record<string, PullRequest[]>>({});
  const lastSyncedAt = ref<number | null>(null);
  const loaded = ref(false);
  const syncing = ref(false);
  const error = ref<AppError | null>(null);

  /** Reads the locally stored pull requests; no network access. */
  async function load(): Promise<void> {
    error.value = null;
    try {
      const [list, mentions] = await Promise.all([
        call("list_pull_requests"),
        call("list_jira_issue_pull_requests"),
      ]);
      pullRequests.value = list.pullRequests;
      lastSyncedAt.value = list.lastSyncedAt;
      const grouped: Record<string, PullRequest[]> = {};
      for (const { issueId, pullRequest } of mentions) {
        (grouped[issueId] ??= []).push(pullRequest);
      }
      byJiraIssue.value = grouped;
    } catch (err: unknown) {
      error.value = err as AppError;
    } finally {
      loaded.value = true;
    }
  }

  /** Fetches from GitHub, then reloads the local list. */
  async function sync(): Promise<GitHubSyncSummary | null> {
    syncing.value = true;
    error.value = null;
    try {
      const summary = await call("sync_pull_requests");
      await load();
      return summary;
    } catch (err: unknown) {
      error.value = err as AppError;
      return null;
    } finally {
      syncing.value = false;
    }
  }

  /**
   * Re-fetches one pull request from GitHub and updates every local copy.
   * Rejects with an `AppError` so the row can show the message.
   */
  async function refresh(id: string): Promise<PullRequest> {
    const updated = await call("refresh_pull_request", { id });
    const replace = (list: PullRequest[]) => {
      const index = list.findIndex((candidate) => candidate.id === id);
      if (index !== -1) list[index] = updated;
    };
    replace(pullRequests.value);
    for (const list of Object.values(byJiraIssue.value)) replace(list);
    return updated;
  }

  /** Reflects a saved workstream change locally, without reloading. */
  function applyWorkstream(id: string, workstreamId: number | null): void {
    const pullRequest = pullRequests.value.find((candidate) => candidate.id === id);
    if (pullRequest) pullRequest.workstreamId = workstreamId;
  }

  async function open(id: string): Promise<void> {
    try {
      await call("open_pull_request", { id });
    } catch (err: unknown) {
      error.value = err as AppError;
    }
  }

  return {
    pullRequests,
    byJiraIssue,
    lastSyncedAt,
    loaded,
    syncing,
    error,
    load,
    sync,
    refresh,
    applyWorkstream,
    open,
  };
});
