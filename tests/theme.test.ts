import { appTheme, illustrationConfig, illustrationPalette, inkFrameStyle, inkShadow, isDarkTheme, SHELL_BACKGROUND } from "@/utils/theme";
import { expect, test } from "vitest";

test("manual themes override the system and system follows it", () => {
  expect(isDarkTheme("dark", false)).toBe(true);
  expect(isDarkTheme("light", true)).toBe(false);
  expect(isDarkTheme("system", true)).toBe(true);
  expect(isDarkTheme("system", false)).toBe(false);
});

test("illustration theme keeps the ink frame and swaps paper in the dark", () => {
  const light = appTheme(false);
  const dark = appTheme(true);
  expect(light.token).toMatchObject({
    colorPrimary: "#52C41A",
    colorBorder: "#2C2C2C",
    colorBgLayout: SHELL_BACKGROUND.light,
    lineWidth: 3,
    borderRadius: 12,
  });
  expect(dark.token).toMatchObject({
    colorPrimary: "#52C41A",
    colorBorder: illustrationPalette(true).ink,
    colorBgLayout: SHELL_BACKGROUND.dark,
    lineWidth: 3,
  });
  expect(dark.token?.colorBorder).not.toBe(light.token?.colorBorder);
  expect(light.components?.Layout).toMatchObject({ bodyBg: "#FFF9F0", lightSiderBg: "#FFF0F6" });
  expect(dark.components?.Layout).toMatchObject({ bodyBg: "#1C1612", lightSiderBg: "#3A2832" });
  expect(light.components?.Card).toMatchObject({ boxShadow: inkShadow("#2C2C2C") });
  expect(dark.components?.Card).toMatchObject({ boxShadow: inkShadow("#F4E7D4") });
  expect(illustrationConfig(true).button?.classNames).toMatchObject({ root: "pixo-ink-button" });
  expect(inkFrameStyle({ lineWidth: 3, colorBorder: "#2C2C2C" }, 16)).toEqual({
    border: "3px solid #2C2C2C",
    borderRadius: 16,
    boxShadow: "4px 4px 0 #2C2C2C",
  });
});
