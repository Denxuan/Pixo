import { migrateSettings } from "@/stores/settings";
import { expect, test } from "vitest";

test("an install saved before first run keeps its tool switches", () => {
  expect(migrateSettings({ mode: "dark", language: "zh-CN" }, 0)).toMatchObject({ introduced: true });
});

test("a saved first-run choice stays as it was", () => {
  expect(migrateSettings({ introduced: false, mode: "system" }, 0)).toMatchObject({ introduced: false });
  expect(migrateSettings({ introduced: true }, 1)).toMatchObject({ introduced: true });
});
