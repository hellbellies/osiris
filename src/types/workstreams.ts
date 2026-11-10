import type { PullRequest } from "./github";
import type { JiraIssue } from "./jira";
import type { ObsidianNote } from "./obsidian";
import type { Todo } from "./todos";

/** Mirrors `Workstream` in `src-tauri/src/models/workstreams.rs`. */
export interface Workstream {
  id: number;
  name: string;
  /** Unix epoch milliseconds. */
  createdAt: number;
  /** Unix epoch milliseconds when marked completed; null while active. */
  completedAt: number | null;
  /** Number of linked items of all types. */
  itemCount: number;
}

/** Mirrors `WorkstreamDetail` in `src-tauri/src/models/workstreams.rs`. */
export interface WorkstreamDetail {
  workstream: Workstream;
  jiraIssues: JiraIssue[];
  pullRequests: PullRequest[];
  todos: Todo[];
  obsidianNotes: ObsidianNote[];
}

/** Mirrors `WorkstreamItemSearch` in `src-tauri/src/models/workstreams.rs`. */
export interface WorkstreamItemSearch {
  jiraIssues: JiraIssue[];
  pullRequests: PullRequest[];
  todos: Todo[];
  obsidianNotes: ObsidianNote[];
  /** Why Obsidian couldn't be searched, e.g. because it isn't running. */
  obsidianError: string | null;
}

/** Mirrors `WorkstreamItemType` in `src-tauri/src/models/workstreams.rs`. */
export type WorkstreamItemType = "jiraIssue" | "pullRequest" | "todo" | "obsidianNote";

/** Mirrors `WorkstreamItemRef` in `src-tauri/src/models/workstreams.rs`. */
export interface WorkstreamItemRef {
  itemType: WorkstreamItemType;
  /** The item's id in its own source, e.g. the Jira issue id, GitHub node id, todo id or `<vault>/<path>`. */
  itemId: string;
}
