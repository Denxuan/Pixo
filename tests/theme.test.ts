import { appTheme, illustrationConfig, illustrationPalette, inkFrameStyle, inkShadow, isDarkTheme, SHELL_BACKGROUND } from "@/utils/theme";
import { expect, test } from "vitest";

test("manual themes override the system and system follows it", () => {
  expect(isDarkTheme("dark", false)).toBe(true);
  expect(isDarkTheme("light", true)).toBe(false);
  expect(isDarkTheme("system", true)).toBe(true);
  expect(isDarkTheme("system", false)).toBe(false);
});

test("illustration theme keeps a quiet frame and swaps paper in the dark", () => {
  const light = appTheme(false);
  const dark = appTheme(true);
  expect(light.token).toMatchObject({
    colorPrimary: "#1677FF",
    colorBorder: "#D5CBBC",
    colorBorderSecondary: "#E4DCD0",
    colorBgLayout: SHELL_BACKGROUND.light,
    lineWidth: 1,
    borderRadius: 8,
  });
  expect(dark.token).toMatchObject({
    colorPrimary: "#1677FF",
    colorBorder: illustrationPalette(true).stroke,
    colorBgLayout: SHELL_BACKGROUND.dark,
    lineWidth: 1,
  });
  expect(dark.token?.colorBorder).not.toBe(light.token?.colorBorder);
  expect(light.token?.colorBorder).not.toBe(light.token?.colorText);
  expect(light.token?.colorBorderSecondary).not.toBe(light.token?.colorBorder);
  expect(light.components?.Layout).toMatchObject({ bodyBg: "#F3EEE6", lightSiderBg: "#E7E0D4" });
  expect(dark.components?.Layout).toMatchObject({ bodyBg: "#100E0C", lightSiderBg: "#141210" });
  expect(light.components?.Card).toMatchObject({ boxShadow: "none" });
  expect(dark.components?.Card).toMatchObject({ boxShadow: "none" });
  expect(illustrationConfig(true).button?.classNames).toMatchObject({ root: "pixo-ink-button" });
  expect(illustrationConfig(true).segmented?.classNames).toMatchObject({ root: "pixo-ink-segmented" });
  expect(inkShadow("#241F1B", 2)).toBe("2px 2px 0 #241F1B");
  expect(inkFrameStyle({ lineWidth: 1, colorBorder: "#D5CBBC" }, 12)).toEqual({
    border: "1px solid #D5CBBC",
    borderRadius: 12,
    boxShadow: "none",
  });
});
