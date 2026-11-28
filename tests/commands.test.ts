import { arrangeLauncherHits, matchCommands } from "@/commands/registry";
import { expect, test } from "vitest";

const ready = { query: "", clipboardOn: true, clipboardPaused: false, screenshotOn: true };

test("an empty search offers actions for tools that are on", () => {
  expect(matchCommands(ready).map((command) => command.id)).toEqual([
    "screenshot.capture",
    "screenshot.record",
    "clipboard.panel",
    "clipboard.pause",
  ]);
});

test("a paused clipboard offers resume instead of pause", () => {
  const ids = matchCommands({ ...ready, clipboardPaused: true }).map((command) => command.id);
  expect(ids).toContain("clipboard.resume");
  expect(ids).not.toContain("clipboard.pause");
});

test("a turned-off tool stays out of search", () => {
  const ids = matchCommands({ ...ready, screenshotOn: false, clipboardOn: false }).map((command) => command.id);
  expect(ids).toEqual([]);
});

test("settings appear when the query names them", () => {
  expect(matchCommands({ ...ready, query: "设置" }).map((command) => command.id)).toEqual(["settings.open"]);
  expect(matchCommands({ ...ready, query: "privacy" }).map((command) => command.id)).toEqual(["settings.data"]);
  expect(matchCommands({ ...ready, query: "sett" }).map((command) => command.id)).toEqual(["settings.open"]);
});

test("a longer search does not turn into a command", () => {
  expect(matchCommands({ ...ready, query: "shot notes" })).toEqual([]);
  expect(matchCommands({ ...ready, query: "database" })).toEqual([]);
  expect(matchCommands({ ...ready, query: "history item" })).toEqual([]);
});

test("typed searches put records ahead of commands", () => {
  const commands = ["capture"];
  const objects = ["host"];
  expect(arrangeLauncherHits("", commands, objects)).toEqual(["capture", "host"]);
  expect(arrangeLauncherHits("s", commands, objects)).toEqual(["host", "capture"]);
});
