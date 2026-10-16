import { invoke } from "@tauri-apps/api/core";

export interface BackupAppearance {
  theme: "system" | "light" | "dark";
  accent: string;
  pure_black: boolean;
}
export interface BackupSummary {
  version: number;
  app_version: string;
  platform: string;
  created_at: string;
  subscriptions: number;
  profiles: number;
  files: number;
  bytes: number;
  appearance: BackupAppearance | null;
}
export const backupPending = () => invoke<boolean>("backup_pending");
export const backupGeneratePassword = () => invoke<string>("backup_generate_password");
export const backupExport = (password: string, appearance: BackupAppearance) =>
  invoke<string>("backup_export", { password, appearance });
export const backupPreview = (content: string, password: string) =>
  invoke<BackupSummary>("backup_preview", { content, password });
export const backupPrepareRestore = (content: string, password: string, appearance: BackupAppearance) =>
  invoke<string>("backup_prepare_restore", { content, password, appearance });
export const backupPrevious = () => invoke<string | null>("backup_previous");
export const backupRestored = () => invoke<BackupSummary | null>("backup_restored");
export const backupAcknowledge = () => invoke<void>("backup_acknowledge");
export const backupRestart = () => invoke<void>("backup_restart");

export async function readBackupFile(file: File): Promise<string> {
  if (file.size > 16 * 1024 * 1024) throw new Error("备份文件不能超过 16 MiB");
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 32768) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
  }
  return btoa(binary);
}
