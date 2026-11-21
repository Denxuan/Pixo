import {
  clampFontSize,
  clampLetterSpacing,
  clampLineHeight,
  clampScrollback,
  defaultSshPalette,
  resolvePalette,
  terminalFontFamily,
} from "@/utils/sshAppearance";
import { expect, test } from "vitest";

test("follow uses ink in dark mode and paper in light mode", () => {
  expect(resolvePalette("follow", defaultSshPalette, true).background).toBe("#101012");
  expect(resolvePalette("follow", defaultSshPalette, false).background).toBe("#fbfbfc");
  expect(resolvePalette("sea", defaultSshPalette, false).cyan).toBe("#7fd3d0");
});

test("a broken custom color falls back to the ink palette", () => {
  const custom = { ...defaultSshPalette, green: "green" };
  expect(resolvePalette("custom", custom, true).green).toBe(defaultSshPalette.green);
});

test("font size, line height, and letter spacing stay in range", () => {
  expect(clampFontSize(3)).toBe(11);
  expect(clampFontSize(40)).toBe(24);
  expect(clampLineHeight(1.26)).toBe(1.3);
  expect(clampLetterSpacing(9)).toBe(6);
  expect(clampScrollback(20)).toBe(1000);
  expect(clampScrollback(250000)).toBe(100000);
  expect(clampScrollback(Number.NaN)).toBe(10000);
});

test("custom font names are quoted and unsafe names fall back", () => {
  expect(terminalFontFamily("custom", "JetBrains Mono")).toBe("'JetBrains Mono', ui-monospace, monospace");
  expect(terminalFontFamily("custom", "Hack'; evil")).toContain("ui-monospace");
  expect(terminalFontFamily("sarasa", "")).toContain("Sarasa Mono SC");
});
