/** Mirrors `JiraSettings` in `src-tauri/src/models/jira.rs`. The token itself never reaches the frontend. */
export interface JiraSettings {
  baseUrl: string;
  email: string;
  projectKeys: string[];
  /** Statuses whose issues are listed whoever they are assigned to. */
  watchedStatuses: JiraStatusRef[];
  hasToken: boolean;
}

/** Mirrors `JiraSettingsInput` in `src-tauri/src/models/jira.rs`. Omit `apiToken` to keep the stored one. */
export interface JiraSettingsInput {
  baseUrl: string;
  email: string;
  projectKeys: string[];
  watchedStatuses: JiraStatusRef[];
  apiToken?: string | null;
}

/** Mirrors `JiraUser` in `src-tauri/src/models/jira.rs`. */
export interface JiraUser {
  displayName: string;
  emailAddress?: string | null;
}

/** Mirrors `JiraIssue` in `src-tauri/src/models/jira.rs`. */
export interface JiraIssue {
  id: string;
  key: string;
  summary: string;
  /** Name of the work type, e.g. "Task", "Bug" or "Epic"; empty if unknown. */
  issueType: string;
  /** ISO-8601 timestamp of the last change in Jira. */
  updated: string;
  projectKey: string;
  statusId: string;
  statusName: string;
  /** False for issues listed because of a watched status, or kept only for a workstream. */
  assignedToMe: boolean;
  /** In a status the user watches, so listed whoever it is assigned to. */
  watched: boolean;
  workstreamId: number | null;
}

/** Mirrors `JiraIssueList` in `src-tauri/src/models/jira.rs`. */
export interface JiraIssueList {
  issues: JiraIssue[];
  /** Unix epoch milliseconds of the last successful sync. */
  lastSyncedAt: number | null;
}

/** Mirrors `JiraSyncMode` in `src-tauri/src/models/jira.rs`. */
export type JiraSyncMode = "full" | "delta";

/** Mirrors `JiraSyncSummary` in `src-tauri/src/models/jira.rs`. */
export interface JiraSyncSummary {
  mode: JiraSyncMode;
  upserted: number;
  removed: number;
  /** Why the related pull requests could not be updated, if they couldn't. */
  pullRequestError: string | null;
}

/** Mirrors `JiraProjectStatus` in `src-tauri/src/models/jira.rs`. */
export interface JiraProjectStatus {
  projectKey: string;
  statusId: string;
  name: string;
  /** Jira status category key: `new`, `indeterminate` or `done`. */
  category: string;
}

/** Mirrors `JiraStatusRef` in `src-tauri/src/models/jira.rs`. */
export interface JiraStatusRef {
  projectKey: string;
  statusId: string;
}

/** Mirrors `JiraStatusFilter` in `src-tauri/src/models/jira.rs`. */
export interface JiraStatusFilter {
  options: JiraProjectStatus[];
  /** Statuses to display. Empty means no filter. */
  selected: JiraStatusRef[];
}
