import { autosaveAction } from "@/hooks/useAutosave";
import { expect, test } from "vitest";

test("the first loaded value is remembered and not written again", () => {
  expect(autosaveAction({ ready: false, inflight: false, saved: null, next: null })).toBe("wait");
  expect(autosaveAction({ ready: false, inflight: false, saved: null, next: "{\"a\":1}" })).toBe("baseline");
  expect(autosaveAction({ ready: true, inflight: false, saved: "{\"a\":1}", next: "{\"a\":1}" })).toBe("wait");
});

test("a changed value is saved, including a revert that overlaps a request", () => {
  expect(autosaveAction({ ready: true, inflight: false, saved: "{\"a\":1}", next: "{\"a\":2}" })).toBe("save");
  expect(autosaveAction({ ready: true, inflight: true, saved: "{\"a\":1}", next: "{\"a\":1}" })).toBe("save");
});
