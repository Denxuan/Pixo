export type DropRect = {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
};

export type SshTransferEvent =
  | { type: "started"; taskId: number; target: string; files: number; total?: number }
  | { type: "progress"; taskId: number; name: string; done: number; total: number }
  | { type: "conflict"; taskId: number; name: string }
  | { type: "file"; taskId: number; name: string; status: "done" | "skipped" | "renamed" | "failed"; code?: string }
  | { type: "finished"; taskId: number; uploaded: number; skipped: number; failed: number; target: string }
  | { type: "failed"; taskId: number; code: string; params?: Record<string, unknown> };

export type TransferRow = {
  taskId: number;
  name: string;
  done: number;
  total: number;
  status: "active" | "done" | "failed";
  uploaded: number;
  skipped: number;
  failedCount: number;
  target: string;
  code: string;
};

export function emptyTransfer(taskId: number): TransferRow {
  return {
    taskId,
    name: "",
    done: 0,
    total: 0,
    status: "active",
    uploaded: 0,
    skipped: 0,
    failedCount: 0,
    target: "",
    code: "",
  };
}

export function reduceTransfer(rows: TransferRow[], event: SshTransferEvent): TransferRow[] {
  if (event.type === "conflict") return rows;
  const index = rows.findIndex((row) => row.taskId === event.taskId);
  const current = index >= 0 ? rows[index] : emptyTransfer(event.taskId);
  let next = current;
  if (event.type === "started") {
    next = { ...current, target: event.target, status: "active", total: event.total ?? current.total, done: 0 };
  } else if (event.type === "progress") {
    next = { ...current, name: event.name, done: event.done, total: event.total };
  } else if (event.type === "file") {
    next = { ...current, name: event.name };
  } else if (event.type === "finished") {
    next = {
      ...current,
      status: "done",
      uploaded: event.uploaded,
      skipped: event.skipped,
      failedCount: event.failed,
      target: event.target,
    };
  } else if (event.type === "failed") {
    next = { ...current, status: "failed", code: event.code };
  }
  const copy = rows.slice();
  if (index >= 0) copy[index] = next;
  else copy.push(next);
  return copy;
}

export function dropHits(rect: DropRect, x: number, y: number, windows: boolean, scale: number): boolean {
  if (rect.width <= 0 || rect.height <= 0) return false;
  const factor = windows && scale > 1 ? scale : 1;
  const px = x / factor;
  const py = y / factor;
  return px >= rect.left && py >= rect.top && px <= rect.right && py <= rect.bottom;
}

export function dropHitsAny(rect: DropRect, x: number, y: number, windows: boolean, scale: number): boolean {
  return dropHits(rect, x, y, windows, scale) || (scale > 1 && dropHits(rect, x, y, !windows, scale));
}

export function transferPercent(done: number, total: number): number {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0 || done <= 0) return 0;
  return Math.min(100, Math.round((done / total) * 100));
}

export function transferClosesItself(task: TransferRow): boolean {
  return task.status === "done" && task.failedCount === 0 && task.uploaded > 0;
}

export function formatBytes(bytes: number, language: string): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(language, { maximumFractionDigits: digits }).format(value)} ${units[unit]}`;
}
