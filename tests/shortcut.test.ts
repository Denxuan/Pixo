import { formatShortcut, shortcutFromEvent } from "@/utils/shortcut";
import { expect, test } from "vitest";

test("records command shift v as a cross platform shortcut", () => {
  expect(shortcutFromEvent({ code: "KeyV", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true })).toBe("CommandOrControl+Shift+V");
});

test("rejects a shortcut without a modifier", () => {
  expect(shortcutFromEvent({ code: "KeyV", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false })).toBeNull();
});

test("formats the stored shortcut for mac and windows", () => {
  expect(formatShortcut("CommandOrControl+Shift+V", true)).toBe("⇧⌘V");
  expect(formatShortcut("CommandOrControl+Shift+V", false)).toBe("Ctrl+Shift+V");
  expect(formatShortcut("Control+Alt+Space", true)).toBe("⌃⌥␣");
});
