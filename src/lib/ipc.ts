import { invoke } from "@tauri-apps/api/core";
import type {
  AppError,
  GitHubSettings,
  GitHubSettingsInput,
  GitHubSyncSummary,
  GitHubUser,
  JiraIssueList,
  JiraIssuePullRequest,
  JiraSettings,
  JiraSettingsInput,
  JiraStatusFilter,
  JiraStatusRef,
  JiraSyncSummary,
  JiraUser,
  PullRequestList,
  Settings,
  Todo,
  TodoInput,
  Workstream,
  WorkstreamDetail,
  WorkstreamItemRef,
  WorkstreamItemSearch,
} from "@/types";
import { isAppError } from "@/types";

/**
 * Registry of every Rust command exposed via `generate_handler!` in
 * `src-tauri/src/lib.rs`. Keys are command names; `args` uses the
 * camelCase names Tauri expects, `result` is the deserialized return type.
 */
interface Commands {
  get_settings: { args: Record<string, never>; result: Settings };
  update_settings: { args: { settings: Settings }; result: Settings };
  get_jira_settings: { args: Record<string, never>; result: JiraSettings };
  update_jira_settings: { args: { input: JiraSettingsInput }; result: JiraSettings };
  clear_jira_token: { args: Record<string, never>; result: JiraSettings };
  test_jira_connection: { args: { input: JiraSettingsInput }; result: JiraUser };
  list_jira_issues: { args: Record<string, never>; result: JiraIssueList };
  sync_jira_issues: { args: { full: boolean }; result: JiraSyncSummary };
  open_jira_issue: { args: { key: string }; result: null };
  get_jira_status_filter: { args: Record<string, never>; result: JiraStatusFilter };
  set_jira_status_filter: { args: { selected: JiraStatusRef[] }; result: JiraStatusFilter };
  get_github_settings: { args: Record<string, never>; result: GitHubSettings };
  update_github_settings: { args: { input: GitHubSettingsInput }; result: GitHubSettings };
  clear_github_token: { args: Record<string, never>; result: GitHubSettings };
  test_github_connection: { args: { token: string | null }; result: GitHubUser };
  list_pull_requests: { args: Record<string, never>; result: PullRequestList };
  list_jira_issue_pull_requests: { args: Record<string, never>; result: JiraIssuePullRequest[] };
  sync_pull_requests: { args: Record<string, never>; result: GitHubSyncSummary };
  open_pull_request: { args: { id: string }; result: null };
  list_todos: { args: Record<string, never>; result: Todo[] };
  create_todo: { args: { input: TodoInput; workstreamId: number | null }; result: Todo };
  update_todo: { args: { id: number; input: TodoInput }; result: Todo };
  set_todo_completed: { args: { id: number; completed: boolean }; result: Todo };
  delete_todo: { args: { id: number }; result: null };
  open_todo_link: { args: { url: string }; result: null };
  list_workstreams: { args: Record<string, never>; result: Workstream[] };
  get_workstream: { args: { id: number }; result: WorkstreamDetail };
  create_workstream: { args: { name: string; items: WorkstreamItemRef[] }; result: Workstream };
  rename_workstream: { args: { id: number; name: string }; result: Workstream };
  set_workstream_completed: { args: { id: number; completed: boolean }; result: Workstream };
  delete_workstream: { args: { id: number }; result: null };
  set_item_workstream: {
    args: { item: WorkstreamItemRef; workstreamId: number | null };
    result: null;
  };
  search_workstream_items: { args: { query: string }; result: WorkstreamItemSearch };
}

type CommandName = keyof Commands;

type ArgsParam<K extends CommandName> =
  Commands[K]["args"] extends Record<string, never> ? [] : [args: Commands[K]["args"]];

/** Typed wrapper around `invoke()`. Rejects with a normalized `AppError`. */
export async function call<K extends CommandName>(
  command: K,
  ...args: ArgsParam<K>
): Promise<Commands[K]["result"]> {
  try {
    return await invoke<Commands[K]["result"]>(command, args[0]);
  } catch (err: unknown) {
    throw toAppError(err);
  }
}

function toAppError(err: unknown): AppError {
  if (isAppError(err)) return err;
  return { kind: "tauri", message: typeof err === "string" ? err : String(err) };
}
