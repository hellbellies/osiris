import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { call } from "@/lib/ipc";
import type {
  AppError,
  PullRequest,
  Workstream,
  WorkstreamDetail,
  WorkstreamItemRef,
  WorkstreamItemSearch,
} from "@/types";

/**
 * Workstreams and the currently opened workstream.
 * Mutations reject with an `AppError` so the calling UI (modal, dropdown, …)
 * can show the message where the user acted.
 */
export const useWorkstreamsStore = defineStore("workstreams", () => {
  const workstreams = ref<Workstream[]>([]);
  const loaded = ref(false);
  const error = ref<AppError | null>(null);

  const active = computed(() => workstreams.value.filter((w) => w.completedAt === null));
  /** Most recently completed first. */
  const completed = computed(() =>
    workstreams.value
      .filter((w) => w.completedAt !== null)
      .sort((a, b) => (b.completedAt ?? 0) - (a.completedAt ?? 0)),
  );

  const current = ref<WorkstreamDetail | null>(null);
  const currentError = ref<AppError | null>(null);

  async function load(): Promise<void> {
    error.value = null;
    try {
      workstreams.value = await call("list_workstreams");
    } catch (err: unknown) {
      error.value = err as AppError;
    } finally {
      loaded.value = true;
    }
  }

  async function loadDetail(id: number): Promise<void> {
    currentError.value = null;
    try {
      current.value = await call("get_workstream", { id });
    } catch (err: unknown) {
      current.value = null;
      currentError.value = err as AppError;
    }
  }

  /** Creates a workstream, moving `items` into it (out of any other workstream). */
  async function create(name: string, items: WorkstreamItemRef[] = []): Promise<Workstream> {
    const workstream = await call("create_workstream", { name, items });
    await load();
    return workstream;
  }

  async function rename(id: number, name: string): Promise<Workstream> {
    const workstream = await call("rename_workstream", { id, name });
    if (current.value?.workstream.id === id) current.value.workstream = workstream;
    await load();
    return workstream;
  }

  /** Marks a workstream completed, or reopens it. */
  async function setCompleted(id: number, isCompleted: boolean): Promise<Workstream> {
    const workstream = await call("set_workstream_completed", { id, completed: isCompleted });
    if (current.value?.workstream.id === id) current.value.workstream = workstream;
    await load();
    return workstream;
  }

  async function remove(id: number): Promise<void> {
    await call("delete_workstream", { id });
    if (current.value?.workstream.id === id) current.value = null;
    await load();
  }

  /** Moves an item into a workstream, or out of its workstream when `workstreamId` is null. */
  async function assign(item: WorkstreamItemRef, workstreamId: number | null): Promise<void> {
    await call("set_item_workstream", { item, workstreamId });
    const openId = current.value?.workstream.id;
    await Promise.all([load(), openId === undefined ? undefined : loadDetail(openId)]);
  }

  /** Reflects a re-fetched pull request in the open workstream, without reloading. */
  function applyPullRequest(pullRequest: PullRequest): void {
    const list = current.value?.pullRequests;
    const index = list?.findIndex((candidate) => candidate.id === pullRequest.id) ?? -1;
    if (list && index !== -1) list[index] = pullRequest;
  }

  /** Jira issues, pull requests and todos whose text contains `query`, to pick items from. */
  function searchItems(query: string): Promise<WorkstreamItemSearch> {
    return call("search_workstream_items", { query });
  }

  return {
    workstreams,
    active,
    completed,
    loaded,
    error,
    current,
    applyPullRequest,
    currentError,
    load,
    loadDetail,
    create,
    rename,
    setCompleted,
    remove,
    assign,
    searchItems,
  };
});
