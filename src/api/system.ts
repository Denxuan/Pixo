import { invoke } from "@tauri-apps/api/core";

import { isDesktop } from "@/utils/desktop";

export const getAutostart = () => invoke<boolean>("system_autostart_enabled");
export const setAutostart = (enabled: boolean) => invoke<void>("system_set_autostart", { enabled });
export const setBackground = (enabled: boolean) => invoke<void>("system_set_background", { enabled });
export const setTrayIcon = (icon: string) => invoke<void>("clipboard_set_tray_icon", { icon });
export const setLauncherShortcut = (shortcut: string) => invoke<void>("launcher_set_shortcut", { shortcut });

export type UpdateStatus = {
  latest: string;
  url: string;
  updateAvailable: boolean;
};

export const checkForUpdate = () => invoke<UpdateStatus>("system_check_update");

export type DataProfile = {
  appDataDir: string;
  clipboardDb: string;
  screenshotLibrary: string;
  screenshotSaveDir: string;
  sshDb: string;
  dockerDb: string;
  recording: boolean;
  sshSessions: number;
  dockerLinks: number;
};

export function dataProfile() {
  if (!isDesktop()) return Promise.reject(new Error("desktop_only"));
  return invoke<DataProfile>("system_data_profile");
}

export function exportBundle(kind: "hosts" | "endpoints", title: string) {
  if (!isDesktop()) return Promise.resolve(false);
  return invoke<boolean>("system_export_bundle", { kind, title });
}
