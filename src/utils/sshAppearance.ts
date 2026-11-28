export const sshPaletteKeys = [
  "background",
  "foreground",
  "cursor",
  "selection",
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const;

export type SshPaletteKey = (typeof sshPaletteKeys)[number];
export type SshPalette = Record<SshPaletteKey, string>;
export type SshThemeId = "follow" | "dark" | "light" | "green" | "solarizedDark" | "solarizedLight" | "dracula" | "nord" | "custom";
export type SshFontId = "system" | "menlo" | "consolas" | "cascadia" | "jetbrains" | "fira" | "sarasa" | "custom";
export type SshCursorStyle = "block" | "bar" | "underline";

export const sshThemes: SshThemeId[] = ["follow", "dark", "light", "green", "solarizedDark", "solarizedLight", "dracula", "nord", "custom"];
export const sshFonts: SshFontId[] = ["system", "menlo", "consolas", "cascadia", "jetbrains", "fira", "sarasa", "custom"];
export const sshCursorStyles: SshCursorStyle[] = ["block", "bar", "underline"];

const darkPalette: SshPalette = {
  background: "#0c0c0c",
  foreground: "#cccccc",
  cursor: "#ffffff",
  selection: "#264f78",
  black: "#0c0c0c",
  red: "#c50f1f",
  green: "#13a10e",
  yellow: "#c19c00",
  blue: "#0037da",
  magenta: "#881798",
  cyan: "#3a96dd",
  white: "#cccccc",
  brightBlack: "#767676",
  brightRed: "#e74856",
  brightGreen: "#16c60c",
  brightYellow: "#f9f1a5",
  brightBlue: "#3b78ff",
  brightMagenta: "#b4009e",
  brightCyan: "#61d6d6",
  brightWhite: "#f2f2f2",
};

const lightPalette: SshPalette = {
  background: "#fafafa",
  foreground: "#383a42",
  cursor: "#526fff",
  selection: "#bfceff",
  black: "#383a42",
  red: "#e45649",
  green: "#50a14f",
  yellow: "#c18401",
  blue: "#0184bc",
  magenta: "#a626a4",
  cyan: "#0997b3",
  white: "#fafafa",
  brightBlack: "#4f525e",
  brightRed: "#e06c75",
  brightGreen: "#98c379",
  brightYellow: "#e5c07b",
  brightBlue: "#61afef",
  brightMagenta: "#c678dd",
  brightCyan: "#56b6c2",
  brightWhite: "#ffffff",
};

const greenPalette: SshPalette = {
  background: "#000000",
  foreground: "#00ff00",
  cursor: "#23ff18",
  selection: "#083905",
  black: "#000000",
  red: "#990000",
  green: "#00a600",
  yellow: "#999900",
  blue: "#0000b2",
  magenta: "#b200b2",
  cyan: "#00a6b2",
  white: "#bfbfbf",
  brightBlack: "#666666",
  brightRed: "#e50000",
  brightGreen: "#00d900",
  brightYellow: "#e5e500",
  brightBlue: "#0000ff",
  brightMagenta: "#e500e5",
  brightCyan: "#00e5e5",
  brightWhite: "#e5e5e5",
};

const solarizedDark: SshPalette = {
  background: "#002b36",
  foreground: "#839496",
  cursor: "#93a1a1",
  selection: "#073642",
  black: "#073642",
  red: "#dc322f",
  green: "#859900",
  yellow: "#b58900",
  blue: "#268bd2",
  magenta: "#d33682",
  cyan: "#2aa198",
  white: "#eee8d5",
  brightBlack: "#002b36",
  brightRed: "#cb4b16",
  brightGreen: "#586e75",
  brightYellow: "#657b83",
  brightBlue: "#839496",
  brightMagenta: "#6c71c4",
  brightCyan: "#93a1a1",
  brightWhite: "#fdf6e3",
};

const solarizedLight: SshPalette = {
  background: "#fdf6e3",
  foreground: "#657b83",
  cursor: "#586e75",
  selection: "#eee8d5",
  black: "#073642",
  red: "#dc322f",
  green: "#859900",
  yellow: "#b58900",
  blue: "#268bd2",
  magenta: "#d33682",
  cyan: "#2aa198",
  white: "#eee8d5",
  brightBlack: "#002b36",
  brightRed: "#cb4b16",
  brightGreen: "#586e75",
  brightYellow: "#657b83",
  brightBlue: "#839496",
  brightMagenta: "#6c71c4",
  brightCyan: "#93a1a1",
  brightWhite: "#fdf6e3",
};

const dracula: SshPalette = {
  background: "#282a36",
  foreground: "#f8f8f2",
  cursor: "#f8f8f2",
  selection: "#44475a",
  black: "#21222c",
  red: "#ff5555",
  green: "#50fa7b",
  yellow: "#f1fa8c",
  blue: "#bd93f9",
  magenta: "#ff79c4",
  cyan: "#8be9fd",
  white: "#f8f8f2",
  brightBlack: "#6272a4",
  brightRed: "#ff6e6e",
  brightGreen: "#69ff94",
  brightYellow: "#ffffa5",
  brightBlue: "#d6acff",
  brightMagenta: "#ff92df",
  brightCyan: "#a4ffff",
  brightWhite: "#ffffff",
};

const nord: SshPalette = {
  background: "#2e3440",
  foreground: "#d8dee9",
  cursor: "#d8dee9",
  selection: "#434c5e",
  black: "#3b4252",
  red: "#bf616a",
  green: "#a3be8c",
  yellow: "#ebcb8b",
  blue: "#81a1c1",
  magenta: "#b48ead",
  cyan: "#88c0d0",
  white: "#e5e9f0",
  brightBlack: "#4c566a",
  brightRed: "#bf616a",
  brightGreen: "#a3be8c",
  brightYellow: "#ebcb8b",
  brightBlue: "#81a1c1",
  brightMagenta: "#b48ead",
  brightCyan: "#8fbcbb",
  brightWhite: "#eceff4",
};

const presets: Record<Exclude<SshThemeId, "follow" | "custom">, SshPalette> = {
  dark: darkPalette,
  light: lightPalette,
  green: greenPalette,
  solarizedDark,
  solarizedLight,
  dracula,
  nord,
};

const legacyThemes: Record<string, SshThemeId> = {
  ink: "dark",
  day: "light",
  sea: "dark",
  grove: "green",
};

export function normalizeThemeId(value: string): SshThemeId {
  if (value in legacyThemes) return legacyThemes[value];
  if ((sshThemes as readonly string[]).includes(value)) return value as SshThemeId;
  return "follow";
}

const fontStacks: Record<Exclude<SshFontId, "custom">, string> = {
  system: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
  menlo: "Menlo, Monaco, ui-monospace, monospace",
  consolas: "Consolas, 'Courier New', monospace",
  cascadia: "'Cascadia Code', 'Cascadia Mono', ui-monospace, monospace",
  jetbrains: "'JetBrains Mono', ui-monospace, monospace",
  fira: "'Fira Code', ui-monospace, monospace",
  sarasa: "'Sarasa Mono SC', 'Sarasa Gothic SC', ui-monospace, monospace",
};

export function isHexColor(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-fA-F]{6}$/.test(value);
}

export function clampFontSize(value: number) {
  if (!Number.isFinite(value)) return 14;
  return Math.min(24, Math.max(11, Math.round(value)));
}

export function clampLineHeight(value: number) {
  if (!Number.isFinite(value)) return 1;
  return Math.min(2, Math.max(1, Math.round(value * 10) / 10));
}

export function clampLetterSpacing(value: number) {
  if (!Number.isFinite(value)) return 0;
  return Math.min(6, Math.max(0, Math.round(value)));
}

export function clampScrollback(value: number) {
  if (!Number.isFinite(value)) return 10000;
  return Math.min(100000, Math.max(1000, Math.round(value)));
}

export function resolvePalette(themeId: SshThemeId | string, custom: SshPalette, dark: boolean): SshPalette {
  const id = normalizeThemeId(themeId);
  const base = id === "follow" ? (dark ? darkPalette : lightPalette) : id === "custom" ? custom : presets[id];
  const fallback = id === "light" || (id === "follow" && !dark) ? lightPalette : darkPalette;
  return sshPaletteKeys.reduce((palette, key) => {
    palette[key] = isHexColor(base[key]) ? base[key] : fallback[key];
    return palette;
  }, {} as SshPalette);
}

export function terminalFontFamily(fontId: SshFontId, customFont: string) {
  if (fontId !== "custom") return fontStacks[fontId];
  const name = customFont.trim();
  if (!/^[\p{L}\p{N} ._-]{1,64}$/u.test(name)) return fontStacks.system;
  return `'${name}', ui-monospace, monospace`;
}

export function toTerminalTheme(palette: SshPalette) {
  return {
    background: palette.background,
    foreground: palette.foreground,
    cursor: palette.cursor,
    cursorAccent: palette.background,
    selectionBackground: palette.selection,
    selectionForeground: palette.foreground,
    black: palette.black,
    red: palette.red,
    green: palette.green,
    yellow: palette.yellow,
    blue: palette.blue,
    magenta: palette.magenta,
    cyan: palette.cyan,
    white: palette.white,
    brightBlack: palette.brightBlack,
    brightRed: palette.brightRed,
    brightGreen: palette.brightGreen,
    brightYellow: palette.brightYellow,
    brightBlue: palette.brightBlue,
    brightMagenta: palette.brightMagenta,
    brightCyan: palette.brightCyan,
    brightWhite: palette.brightWhite,
  };
}

export const defaultSshPalette = darkPalette;
