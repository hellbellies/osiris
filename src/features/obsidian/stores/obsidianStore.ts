import { defineStore } from "pinia";
import { ref } from "vue";
import { call } from "@/lib/ipc";
import type { ObsidianNote, ObsidianStatus } from "@/types";

/** Whether Obsidian is set up for notes, plus note search and opening. */
export const useObsidianStore = defineStore("obsidian", () => {
  const status = ref<ObsidianStatus | null>(null);
  const checking = ref(false);

  /** Re-checks the prerequisites; never starts Obsidian. */
  async function checkStatus(): Promise<ObsidianStatus | null> {
    checking.value = true;
    try {
      status.value = await call("get_obsidian_status");
    } catch {
      status.value = null;
    } finally {
      checking.value = false;
    }
    return status.value;
  }

  /** Notes in the open vaults; rejects with an `AppError` (e.g. Obsidian not running). */
  function search(query: string): Promise<ObsidianNote[]> {
    return call("search_obsidian_notes", { query });
  }

  /** Opens a note in Obsidian; rejects with an `AppError`. */
  async function open(id: string): Promise<void> {
    await call("open_obsidian_note", { id });
  }

  return { status, checking, checkStatus, search, open };
});
