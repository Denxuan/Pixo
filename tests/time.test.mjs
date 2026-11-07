import assert from "node:assert/strict";
import { test } from "node:test";

import { bytesToMegabytes, formatBytes, formatRelativeTime, megabytesToBytes } from "../src/utils/time.ts";

test("formats byte sizes with localized units", () => {
  assert.match(formatBytes(1536, "en"), /1\.5/);
  assert.match(formatBytes(1024 * 1024, "zh-CN"), /1/);
  assert.equal(formatBytes(12, "en").includes("12"), true);
});

test("formats a fixed timestamp relative to now", () => {
  const now = Date.UTC(2026, 10, 7, 2, 0, 0);
  assert.match(formatRelativeTime(now - 5_000, "en", now), /second/i);
  assert.match(formatRelativeTime(now - 5_000, "zh-CN", now), /秒/);
  assert.match(formatRelativeTime(now - 3_600_000 * 3, "en", now), /hour/i);
});

test("converts megabytes and bytes", () => {
  assert.equal(megabytesToBytes("1"), 1024 * 1024);
  assert.equal(bytesToMegabytes(20 * 1024 * 1024), "20");
});
