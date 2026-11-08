import { expect, test, vi } from "vitest";

test("preserves existing theme while storing background mode and language", async () => {
  const values = new Map([["pixo-theme", JSON.stringify({ state: { mode: "dark", language: "en" }, version: 0 })]]);
  const localStorage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  };
  vi.stubGlobal("window", { localStorage });
  const { useSettingsStore } = await import("@/stores/settings");
  await useSettingsStore.persist.rehydrate();
  expect(useSettingsStore.getState().mode).toBe("dark");
  expect(useSettingsStore.getState().background).toBe(false);
  useSettingsStore.getState().setBackground(true);
  const saved = JSON.parse(values.get("pixo-theme")!);
  expect(saved.state).toMatchObject({ mode: "dark", language: "en", background: true });
  vi.unstubAllGlobals();
});
