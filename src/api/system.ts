import { invoke } from "@tauri-apps/api/core";

export const getAutostart = () => invoke<boolean>("system_autostart_enabled");
export const setAutostart = (enabled: boolean) => invoke<void>("system_set_autostart", { enabled });
export const setBackground = (enabled: boolean) => invoke<void>("system_set_background", { enabled });
export const setTrayIcon = (icon: string) => invoke<void>("clipboard_set_tray_icon", { icon });

export type UpdateStatus = {
  latest: string;
  url: string;
  updateAvailable: boolean;
};

export const checkForUpdate = () => invoke<UpdateStatus>("system_check_update");
