import { expect, test } from "vitest";

import { dropHits, dropHitsAny, formatBytes, reduceTransfer, transferClosesItself, transferPercent, type SshTransferEvent, type TransferRow } from "@/utils/sshTransfer";

test("tracks one upload from start to finish", () => {
    const events: SshTransferEvent[] = [
      { type: "started", taskId: 4, target: "/opt/zdx", files: 1, total: 10 },
      { type: "progress", taskId: 4, name: "note.txt", done: 5, total: 10 },
      { type: "finished", taskId: 4, uploaded: 1, skipped: 0, failed: 0, target: "/opt/zdx" },
    ];
    const row = events.reduce<TransferRow[]>(reduceTransfer, [])[0];
    expect(row.status).toBe("done");
    expect(row.name).toBe("note.txt");
    expect(row.uploaded).toBe(1);
    expect(row.target).toBe("/opt/zdx");
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

test("formats byte sizes", () => {
    expect(formatBytes(0, "zh-CN")).toBe("0 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
  });
