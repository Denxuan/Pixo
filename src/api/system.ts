import { invoke } from "@tauri-apps/api/core";

export const getAutostart = () => invoke<boolean>("system_autostart_enabled");
export const setAutostart = (enabled: boolean) => invoke<void>("system_set_autostart", { enabled });
export const setBackground = (enabled: boolean) => invoke<void>("system_set_background", { enabled });
