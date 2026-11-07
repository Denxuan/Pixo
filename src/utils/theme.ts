import type { ThemeMode } from "../stores/settings";

export function isDarkTheme(mode: ThemeMode, systemDark: boolean): boolean {
  return mode === "dark" || (mode === "system" && systemDark);
}
