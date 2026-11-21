import { clampHostPane, DEFAULT_HOST_PANE, readHostPaneWidth } from "@/utils/sshLayout";
import { expect, test } from "vitest";

test("a saved host pane width stays inside the stored range", () => {
  expect(readHostPaneWidth(null)).toBe(DEFAULT_HOST_PANE);
  expect(readHostPaneWidth("")).toBe(DEFAULT_HOST_PANE);
  expect(readHostPaneWidth("wide")).toBe(DEFAULT_HOST_PANE);
  expect(readHostPaneWidth("40")).toBe(200);
  expect(readHostPaneWidth("400")).toBe(400);
  expect(readHostPaneWidth("9000")).toBe(1600);
});

test("dragging keeps a terminal area and still fits a narrow window", () => {
  expect(clampHostPane(288, 1000)).toBe(288);
  expect(clampHostPane(80, 1000)).toBe(200);
  expect(clampHostPane(900, 1000)).toBe(680);
  expect(clampHostPane(Number.NaN, 1000)).toBe(DEFAULT_HOST_PANE);
  expect(clampHostPane(400, 420)).toBe(160);
});
