import { bytesToMegabytes, formatBytes, formatRelativeTime, megabytesToBytes } from "@/utils/time";
import { expect, test } from "vitest";

test("formats byte sizes with localized units", () => {
  expect(formatBytes(1536, "en")).toMatch(/1\.5/);
  expect(formatBytes(1024 * 1024, "zh-CN")).toMatch(/1/);
  expect(formatBytes(12, "en").includes("12")).toBe(true);
});

test("formats a fixed timestamp relative to now", () => {
  const now = Date.UTC(2026, 10, 7, 2, 0, 0);
  expect(formatRelativeTime(now - 5_000, "en", now)).toMatch(/second/i);
  expect(formatRelativeTime(now - 5_000, "zh-CN", now)).toMatch(/秒/);
  expect(formatRelativeTime(now - 3_600_000 * 3, "en", now)).toMatch(/hour/i);
});

test("converts megabytes and bytes", () => {
  expect(megabytesToBytes("1")).toBe(1024 * 1024);
  expect(bytesToMegabytes(20 * 1024 * 1024)).toBe("20");
});
