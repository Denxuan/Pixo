import {
  clampFontSize,
  clampLetterSpacing,
  clampLineHeight,
  clampScrollback,
  defaultSshPalette,
  isHexColor,
  type SshCursorStyle,
  type SshFontId,
  type SshPalette,
  type SshPaletteKey,
  type SshThemeId,
} from "@/utils/sshAppearance";

type SshAppearanceState = {
  themeId: SshThemeId;
  custom: SshPalette;
  fontId: SshFontId;
  customFont: string;
  fontSize: number;
  lineHeight: number;
  letterSpacing: number;
  cursorStyle: SshCursorStyle;
  cursorBlink: boolean;
  contrast: boolean;
  scrollback: number;
  confirmPaste: boolean;
  copySkipsHistory: boolean;
  setThemeId: (themeId: SshThemeId) => void;
  setCustomColor: (key: SshPaletteKey, value: string) => void;
  beginCustom: (palette: SshPalette) => void;
  setFontId: (fontId: SshFontId) => void;
  setCustomFont: (customFont: string) => void;
  setFontSize: (fontSize: number) => void;
  setLineHeight: (lineHeight: number) => void;
  setLetterSpacing: (letterSpacing: number) => void;
  setCursorStyle: (cursorStyle: SshCursorStyle) => void;
  setCursorBlink: (cursorBlink: boolean) => void;
  setContrast: (contrast: boolean) => void;
  setScrollback: (scrollback: number) => void;
  setConfirmPaste: (confirmPaste: boolean) => void;
  setCopySkipsHistory: (copySkipsHistory: boolean) => void;
  resetAppearance: () => void;
};

const initial = {
  themeId: "follow" as SshThemeId,
  custom: defaultSshPalette,
  fontId: "system" as SshFontId,
  customFont: "",
  fontSize: 14,
  lineHeight: 1,
  letterSpacing: 0,
  cursorStyle: "block" as SshCursorStyle,
  cursorBlink: true,
  contrast: false,
  scrollback: 10000,
  confirmPaste: true,
  copySkipsHistory: true,
};

export const useSshAppearanceStore = createZustandStore<SshAppearanceState>()(persist((set) => ({
  ...initial,
  custom: { ...defaultSshPalette },
  setThemeId: (themeId) => set({ themeId }),
  setCustomColor: (key, value) => set((state) => isHexColor(value) ? { custom: { ...state.custom, [key]: value } } : state),
  beginCustom: (palette) => set({ themeId: "custom", custom: { ...palette } }),
  setFontId: (fontId) => set({ fontId }),
  setCustomFont: (customFont) => set({ customFont: customFont.slice(0, 64) }),
  setFontSize: (fontSize) => set({ fontSize: clampFontSize(fontSize) }),
  setLineHeight: (lineHeight) => set({ lineHeight: clampLineHeight(lineHeight) }),
  setLetterSpacing: (letterSpacing) => set({ letterSpacing: clampLetterSpacing(letterSpacing) }),
  setCursorStyle: (cursorStyle) => set({ cursorStyle }),
  setCursorBlink: (cursorBlink) => set({ cursorBlink }),
  setContrast: (contrast) => set({ contrast }),
  setScrollback: (scrollback) => set({ scrollback: clampScrollback(scrollback) }),
  setConfirmPaste: (confirmPaste) => set({ confirmPaste }),
  setCopySkipsHistory: (copySkipsHistory) => set({ copySkipsHistory }),
  resetAppearance: () => set({ ...initial, custom: { ...defaultSshPalette } }),
}), { name: "pixo-ssh-terminal" }));
