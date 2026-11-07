import { isDarkTheme } from "@/utils/theme";
import { expect, test } from "vitest";

test("manual themes override the system and system follows it", () => {
  expect(isDarkTheme("dark", false)).toBe(true);
  expect(isDarkTheme("light", true)).toBe(false);
  expect(isDarkTheme("system", true)).toBe(true);
  expect(isDarkTheme("system", false)).toBe(false);
});
