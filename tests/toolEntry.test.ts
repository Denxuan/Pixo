import { homeForTools, redirectForClosedTool } from "@/utils/toolEntry";
import { expect, test } from "vitest";

const ready = { clipboard: true, screenshot: false, ssh: false, docker: false };

test("the home page is the first tool that is on", () => {
  expect(homeForTools({ clipboard: null, screenshot: false, ssh: false, docker: false })).toBeNull();
  expect(homeForTools(ready)).toBe("/clipboard");
  expect(homeForTools({ ...ready, clipboard: false, screenshot: true })).toBe("/screenshot");
  expect(homeForTools({ ...ready, clipboard: false, ssh: true })).toBe("/ssh");
  expect(homeForTools({ clipboard: false, screenshot: false, ssh: false, docker: false })).toBe("/settings");
});

test("a closed tool page moves to a tool that is still on", () => {
  expect(redirectForClosedTool("/clipboard", ready)).toBeNull();
  expect(redirectForClosedTool("/screenshot", { ...ready, clipboard: false, screenshot: false, ssh: true })).toBe("/ssh");
  expect(redirectForClosedTool("/clipboard", { clipboard: false, screenshot: false, ssh: false, docker: false })).toBe("/settings");
  expect(redirectForClosedTool("/settings", ready)).toBeNull();
});
