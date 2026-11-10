/** Mirrors `AppError` in `src-tauri/src/error.rs`. */
export type AppErrorKind = "io" | "serde" | "tauri" | "keyring" | "validation" | "jira" | "github" | "obsidian" | "db";

export interface AppError {
  kind: AppErrorKind;
  message: string;
}

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value &&
    typeof (value as AppError).message === "string"
  );
}
