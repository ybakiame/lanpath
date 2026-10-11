import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

export const PROJECT_URL = "https://github.com/ybakiame/lanpath";

/** 读取壳的实际应用版本，避免 UI 常量与安装包版本漂移。 */
export const getAppVersion = getVersion;

/** 桌面便携版仅从发布页手动更新，避免执行安装版 updater。 */
export function isPortable(): Promise<boolean> {
  return invoke<boolean>("is_portable");
}

/** 通过原生 opener 在系统浏览器中打开项目主页。 */
export function openProjectUrl(): Promise<void> {
  return openUrl(PROJECT_URL);
}

/** Android 更新通过发布页分发，不调用桌面 updater 插件。 */
export function openReleaseUrl(): Promise<void> {
  return openUrl(`${PROJECT_URL}/releases`);
}
