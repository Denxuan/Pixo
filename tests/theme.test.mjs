import { test } from "node:test";
import assert from "node:assert/strict";
import { isDarkTheme } from "../src/utils/theme.ts";

test("manual themes override the system and system follows it", () => {
  assert.equal(isDarkTheme("dark", false), true);
  assert.equal(isDarkTheme("light", true), false);
  assert.equal(isDarkTheme("system", true), true);
  assert.equal(isDarkTheme("system", false), false);
});
