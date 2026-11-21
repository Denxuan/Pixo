import { sshTerminalShortcut } from "@/utils/sshTerminalKey";
import { expect, test } from "vitest";

const down = { type: "keydown", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false };

test("terminal shortcuts run on keydown and not again on keyup", () => {
  expect(sshTerminalShortcut({ ...down, key: "v", metaKey: true })).toBe("paste");
  expect(sshTerminalShortcut({ ...down, type: "keyup", key: "v", metaKey: true })).toBeNull();
  expect(sshTerminalShortcut({ ...down, key: "c", metaKey: true })).toBe("copy");
  expect(sshTerminalShortcut({ ...down, type: "keyup", key: "c", metaKey: true })).toBeNull();
  expect(sshTerminalShortcut({ ...down, key: "v", ctrlKey: true, shiftKey: true })).toBe("paste");
  expect(sshTerminalShortcut({ ...down, key: "f", ctrlKey: true })).toBe("find");
  expect(sshTerminalShortcut({ ...down, key: "e", metaKey: true, shiftKey: true })).toBe("leave");
  expect(sshTerminalShortcut({ ...down, key: "a", metaKey: true })).toBeNull();
});
