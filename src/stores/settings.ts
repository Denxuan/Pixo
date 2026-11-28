import { isTrayIcon, type TrayIconId } from "@/components/icons/TrayMark";

export type ThemeMode = "system" | "light" | "dark";
export type Language = "zh-CN" | "en";

export function migrateSettings(persisted: unknown, version: number): Record<string, unknown> {
  const state = persisted && typeof persisted === "object" ? { ...(persisted as Record<string, unknown>) } : {};
  // Installs saved before the first-run flag already chose their tools.
  if (version < 1 && !(persisted && typeof persisted === "object" && "introduced" in persisted)) {
    state.introduced = true;
  }
  return state;
}

export const useSettingsStore = createZustandStore<{
  mode: ThemeMode;
  language: Language;
  background: boolean;
  trayIcon: TrayIconId;
  introduced: boolean;
  launcherShortcut: string;
  setMode: (mode: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setBackground: (background: boolean) => void;
  setTrayIcon: (trayIcon: TrayIconId) => void;
  setIntroduced: (introduced: boolean) => void;
  setLauncherShortcut: (launcherShortcut: string) => void;
}>()(persist((set) => ({
  mode: "system",
  language: "zh-CN",
  background: false,
  trayIcon: "board",
  introduced: false,
  launcherShortcut: "CommandOrControl+Shift+Space",
  setMode: (mode) => set({ mode }),
  setLanguage: (language) => set({ language }),
  setBackground: (background) => set({ background }),
  setTrayIcon: (trayIcon) => set({ trayIcon }),
  setIntroduced: (introduced) => set({ introduced }),
  setLauncherShortcut: (launcherShortcut) => set({ launcherShortcut }),
// Keep the existing key so users retain their saved theme.
}), { name: "pixo-theme", version: 1, migrate: migrateSettings }));

export function useTrayIcon(): TrayIconId {
  const trayIcon = useSettingsStore((state) => state.trayIcon);
  return isTrayIcon(trayIcon) ? trayIcon : "board";
}
