export type ThemeMode = "system" | "light" | "dark";
export type Language = "zh-CN" | "en";

export const useSettingsStore = createZustandStore<{
  mode: ThemeMode;
  language: Language;
  background: boolean;
  setMode: (mode: ThemeMode) => void;
  setLanguage: (language: Language) => void;
  setBackground: (background: boolean) => void;
}>()(persist((set) => ({
  mode: "system",
  language: "zh-CN",
  background: false,
  setMode: (mode) => set({ mode }),
  setLanguage: (language) => set({ language }),
  setBackground: (background) => set({ background }),
// Keep the existing key so users retain their saved theme.
}), { name: "pixo-theme" }));
