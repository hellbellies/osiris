/** Mirrors `ObsidianState` in `src-tauri/src/models/obsidian.rs`. */
export type ObsidianState =
  | "notInstalled"
  | "outdated"
  | "notRunning"
  | "cliDisabled"
  | "ready"
  | "failed";

/** Mirrors `ObsidianVault` in `src-tauri/src/models/obsidian.rs`. */
export interface ObsidianVault {
  name: string;
  path: string;
  /** Open in an Obsidian window; only open vaults are searched. */
  open: boolean;
}

/** Mirrors `ObsidianStatus` in `src-tauri/src/models/obsidian.rs`. */
export interface ObsidianStatus {
  state: ObsidianState;
  version: string | null;
  vaults: ObsidianVault[];
  /** Details when `state` is "failed". */
  message: string | null;
}

/** Mirrors `ObsidianNote` in `src-tauri/src/models/obsidian.rs`. */
export interface ObsidianNote {
  /** `<vault>/<path>`; the item id in workstreams. */
  id: string;
  vault: string;
  path: string;
  /** File name without `.md`. */
  title: string;
  /** Folder inside the vault; empty at the top level. */
  folder: string;
  workstreamId: number | null;
}
