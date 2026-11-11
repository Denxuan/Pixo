import { eventMatchesShortcut, formatShortcut, shortcutFromEvent } from "@/utils/shortcut";
import { expect, test } from "vitest";

test("records command shift v as a cross platform shortcut", () => {
  expect(shortcutFromEvent({ code: "KeyV", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true })).toBe("CommandOrControl+Shift+V");
});

test("rejects a shortcut without a modifier", () => {
  expect(shortcutFromEvent({ code: "KeyV", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false })).toBeNull();
});

test("matches the selection shortcuts on mac and windows", () => {
  const pin = { code: "KeyP", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
  const remove = { code: "Backspace", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false };
  expect(eventMatchesShortcut(pin, "CommandOrControl+P", true)).toBe(true);
  expect(eventMatchesShortcut({ ...pin, metaKey: false, ctrlKey: true }, "CommandOrControl+P", false)).toBe(true);
  expect(eventMatchesShortcut(remove, "CommandOrControl+Backspace", false)).toBe(true);
  expect(eventMatchesShortcut(pin, "CommandOrControl+Backspace", true)).toBe(false);
  expect(eventMatchesShortcut({ ...pin, shiftKey: true }, "CommandOrControl+P", true)).toBe(false);
});

test("formats the stored shortcut for mac and windows", () => {
  expect(formatShortcut("CommandOrControl+Shift+V", true)).toBe("⇧⌘V");
  expect(formatShortcut("CommandOrControl+Shift+V", false)).toBe("Ctrl+Shift+V");
  expect(formatShortcut("Control+Alt+Space", true)).toBe("⌃⌥␣");
});
