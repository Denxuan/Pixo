export type ThemeMode = "system" | "light" | "dark";
export type Language = "zh-CN" | "en";

export const useSettingsStore = createZustandStore<{
  mode: ThemeMode;
  language: Language;
  setMode: (mode: ThemeMode) => void;
  setLanguage: (language: Language) => void;
}>()(persist((set) => ({
  mode: "system",
  language: "zh-CN",
  setMode: (mode) => set({ mode }),
  setLanguage: (language) => set({ language }),
// Keep the existing key so users retain their saved theme.
}), { name: "pixo-theme" }));
