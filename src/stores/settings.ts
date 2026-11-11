import { isTrayIcon, type TrayIconId } from "@/components/icons/TrayMark";

export type ThemeMode = "system" | "light" | "dark";
export type Language = "zh-CN" | "en";

export const useSettingsStore = createZustandStore<{
  mode: ThemeMode;
  language: Language;
  background: boolean;
  trayIcon: TrayIconId;
  setMode: (mode: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setBackground: (background: boolean) => void;
  setTrayIcon: (trayIcon: TrayIconId) => void;
}>()(persist((set) => ({
  mode: "system",
  language: "zh-CN",
  background: false,
  trayIcon: "board",
  setMode: (mode) => set({ mode }),
  setLanguage: (language) => set({ language }),
  setBackground: (background) => set({ background }),
  setTrayIcon: (trayIcon) => set({ trayIcon }),
// Keep the existing key so users retain their saved theme.
}), { name: "pixo-theme" }));

export function useTrayIcon(): TrayIconId {
  const trayIcon = useSettingsStore((state) => state.trayIcon);
  return isTrayIcon(trayIcon) ? trayIcon : "board";
}
