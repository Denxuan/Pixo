import { expect, test } from "vitest";

import { dropHits, dropHitsAny, formatBytes, formatRate, formatRemaining, reduceTransfer, transferCanRetry, transferClosesItself, transferPace, transferPercent, type SshTransferEvent, type TransferRow } from "@/utils/sshTransfer";

test("tracks one upload from start to finish", () => {
    const events: SshTransferEvent[] = [
      { type: "started", taskId: 4, target: "/opt/zdx", files: 1, total: 10 },
      { type: "progress", taskId: 4, name: "note.txt", done: 5, total: 10 },
      { type: "finished", taskId: 4, uploaded: 1, skipped: 0, failed: 0, target: "/opt/zdx" },
    ];
    const row = events.reduce<TransferRow[]>((rows, event) => reduceTransfer(rows, event, 1_000), [])[0];
    expect(row.status).toBe("done");
    expect(row.name).toBe("note.txt");
    expect(row.uploaded).toBe(1);
    expect(row.target).toBe("/opt/zdx");
    expect(row.direction).toBe("upload");
    expect(transferPercent(5, 10)).toBe(50);
    expect(transferPercent(0, 10)).toBe(0);
    expect(transferClosesItself(row)).toBe(true);
    expect(transferClosesItself({ ...row, failedCount: 1 })).toBe(false);
  });

test("hits the terminal in css pixels and windows physical pixels", () => {
    const rect = { left: 300, top: 80, right: 900, bottom: 600, width: 600, height: 520 };
    expect(dropHits(rect, 400, 200, false, 2)).toBe(true);
    expect(dropHits(rect, 100, 200, false, 2)).toBe(false);
    expect(dropHits(rect, 800, 400, true, 2)).toBe(true);
    expect(dropHits({ ...rect, width: 0, height: 0 }, 400, 200, false, 1)).toBe(false);
    expect(dropHits(rect, 1000, 200, false, 2)).toBe(false);
    expect(dropHitsAny(rect, 1000, 200, false, 2)).toBe(true);
    expect(dropHitsAny(rect, 100, 200, false, 2)).toBe(false);
  });

test("keeps a download labeled separately from an upload", () => {
  const rows = reduceTransfer([], {
    type: "started",
    taskId: 8,
    target: "Downloads",
    files: 2,
    total: 20,
    direction: "download",
  });
  expect(rows[0].direction).toBe("download");
  expect(rows[0].target).toBe("Downloads");
});

test("formats byte sizes", () => {
    expect(formatBytes(0, "zh-CN")).toBe("0 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatRate(1536, "en")).toBe("1.5 KB/s");
    expect(formatRemaining(65, "en")).toBe("1:05");
    expect(formatRemaining(3661, "en")).toBe("1:01:01");
    expect(formatRemaining(0.2, "en")).toBe("0:01");
  });

test("estimates speed from recent progress and leaves a stalled transfer without a time", () => {
  const moving = transferPace([{ at: 0, done: 0 }, { at: 1000, done: 1000 }], 1000, 5000);
  expect(moving.rate).toBe(1000);
  expect(moving.remaining).toBe(4);
  expect(transferPace([{ at: 0, done: 0 }, { at: 1000, done: 1000 }], 4000, 5000).rate).toBe(0);
  expect(transferPace([{ at: 0, done: 0 }], 200, 100).remaining).toBeNull();
});

test("keeps a failed transfer retryable and resets the pace when it starts again", () => {
  const failed = reduceTransfer([], { type: "failed", taskId: 3, code: "disconnected" }, 0);
  expect(transferCanRetry(failed[0])).toBe(true);
  const partial = reduceTransfer(failed, { type: "finished", taskId: 3, uploaded: 1, skipped: 0, failed: 2, target: "/tmp" }, 10);
  expect(transferCanRetry(partial[0])).toBe(true);
  expect(transferCanRetry({ ...partial[0], status: "done", failedCount: 0 })).toBe(false);
  const restarted = reduceTransfer([], { type: "started", taskId: 9, target: "/tmp", files: 1, total: 100 }, 50);
  const moving = reduceTransfer(restarted, { type: "progress", taskId: 9, name: "a.bin", done: 40, total: 100 }, 1050);
  expect(moving[0].samples).toEqual([{ at: 50, done: 0 }, { at: 1050, done: 40 }]);
  expect(transferPace(moving[0].samples, 1050, 100).rate).toBe(40);
});
