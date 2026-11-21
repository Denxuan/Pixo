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
export type SshThemeId = "follow" | "ink" | "day" | "sea" | "grove" | "custom";
export type SshFontId = "system" | "menlo" | "consolas" | "cascadia" | "jetbrains" | "fira" | "sarasa" | "custom";
export type SshCursorStyle = "block" | "bar" | "underline";

export const sshThemes: SshThemeId[] = ["follow", "ink", "day", "sea", "grove", "custom"];
export const sshFonts: SshFontId[] = ["system", "menlo", "consolas", "cascadia", "jetbrains", "fira", "sarasa", "custom"];
export const sshCursorStyles: SshCursorStyle[] = ["block", "bar", "underline"];

const ink: SshPalette = {
  background: "#101012",
  foreground: "#e8e8ed",
  cursor: "#e8e8ed",
  selection: "#3d4ad6",
  black: "#3a3a40",
  red: "#ff6b6b",
  green: "#3dd68c",
  yellow: "#f5c451",
  blue: "#7aa2f7",
  magenta: "#c792ea",
  cyan: "#5fd0d0",
  white: "#d6d6dc",
  brightBlack: "#6e6e76",
  brightRed: "#ff8d8d",
  brightGreen: "#6ee7ad",
  brightYellow: "#ffd56a",
  brightBlue: "#9cbcff",
  brightMagenta: "#ddb4f5",
  brightCyan: "#8ae4e4",
  brightWhite: "#ffffff",
};

const day: SshPalette = {
  background: "#fbfbfc",
  foreground: "#1c1c1e",
  cursor: "#1c1c1e",
  selection: "#d5dbff",
  black: "#2a2a2e",
  red: "#b42318",
  green: "#067647",
  yellow: "#b54708",
  blue: "#1d4ed8",
  magenta: "#9d174d",
  cyan: "#0e7490",
  white: "#f4f4f6",
  brightBlack: "#52525b",
  brightRed: "#d92d20",
  brightGreen: "#079455",
  brightYellow: "#dc6803",
  brightBlue: "#3b6cff",
  brightMagenta: "#c026d3",
  brightCyan: "#06b6d4",
  brightWhite: "#ffffff",
};

const sea: SshPalette = {
  background: "#0e1c24",
  foreground: "#d5e7ea",
  cursor: "#9be7e0",
  selection: "#1d6f7a",
  black: "#16323a",
  red: "#e56b6b",
  green: "#6dcf9a",
  yellow: "#e3c36a",
  blue: "#7eb6d9",
  magenta: "#d39ad6",
  cyan: "#7fd3d0",
  white: "#d5e7ea",
  brightBlack: "#4d6d76",
  brightRed: "#ff8f8f",
  brightGreen: "#8ee3b4",
  brightYellow: "#f3d98a",
  brightBlue: "#a5d0ea",
  brightMagenta: "#e4b8e6",
  brightCyan: "#b6f0ee",
  brightWhite: "#ffffff",
};

const grove: SshPalette = {
  background: "#141a14",
  foreground: "#e4ecdf",
  cursor: "#c6e2a4",
  selection: "#3d5c32",
  black: "#243024",
  red: "#e07a6a",
  green: "#8fce6a",
  yellow: "#d6c36a",
  blue: "#8eb4c9",
  magenta: "#c9a0c4",
  cyan: "#7ec8b0",
  white: "#e4ecdf",
  brightBlack: "#5d6b58",
  brightRed: "#f09a8c",
  brightGreen: "#b4e492",
  brightYellow: "#ead48e",
  brightBlue: "#b4d2e2",
  brightMagenta: "#e0c0dc",
  brightCyan: "#a8e4d0",
  brightWhite: "#ffffff",
};

const presets: Record<Exclude<SshThemeId, "follow" | "custom">, SshPalette> = { ink, day, sea, grove };

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

export function resolvePalette(themeId: SshThemeId, custom: SshPalette, dark: boolean): SshPalette {
  const base = themeId === "follow" ? (dark ? ink : day) : themeId === "custom" ? custom : presets[themeId];
  const fallback = themeId === "day" || (themeId === "follow" && !dark) ? day : ink;
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

export const defaultSshPalette = ink;
