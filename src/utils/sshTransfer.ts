export type DropRect = {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
};

export type SshTransferEvent =
  | { type: "started"; taskId: number; target: string; files: number; total?: number; direction?: "upload" | "download" }
  | { type: "progress"; taskId: number; name: string; done: number; total: number }
  | { type: "conflict"; taskId: number; name: string }
  | { type: "file"; taskId: number; name: string; status: "done" | "skipped" | "renamed" | "failed"; code?: string }
  | { type: "finished"; taskId: number; uploaded: number; skipped: number; failed: number; target: string; direction?: "upload" | "download" }
  | { type: "failed"; taskId: number; code: string; params?: Record<string, unknown> };

export type PaceSample = { at: number; done: number };

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
  direction: "upload" | "download";
  samples: PaceSample[];
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
    direction: "upload",
    samples: [],
  };
}

function rememberPace(samples: PaceSample[], at: number, done: number): PaceSample[] {
  const cutoff = at - 8000;
  const next = samples.filter((sample) => sample.at >= cutoff);
  next.push({ at, done });
  return next.slice(-40);
}

export function reduceTransfer(rows: TransferRow[], event: SshTransferEvent, now = Date.now()): TransferRow[] {
  if (event.type === "conflict") return rows;
  const index = rows.findIndex((row) => row.taskId === event.taskId);
  const current = index >= 0 ? rows[index] : emptyTransfer(event.taskId);
  let next = current;
  if (event.type === "started") {
    next = {
      ...current,
      target: event.target,
      status: "active",
      total: event.total ?? current.total,
      done: 0,
      direction: event.direction ?? current.direction,
      samples: [{ at: now, done: 0 }],
    };
  } else if (event.type === "progress") {
    next = {
      ...current,
      name: event.name,
      done: event.done,
      total: event.total,
      samples: rememberPace(current.samples, now, event.done),
    };
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

export function transferCanRetry(task: TransferRow): boolean {
  if (task.status === "failed") return true;
  return task.status === "done" && task.failedCount > 0;
}

export function transferPace(samples: PaceSample[], now: number, total: number): { rate: number; remaining: number | null } {
  if (samples.length < 2 || !Number.isFinite(now)) return { rate: 0, remaining: null };
  const latest = samples[samples.length - 1];
  if (!latest || now - latest.at > 2000) return { rate: 0, remaining: null };
  const cutoff = latest.at - 4000;
  const first = samples.find((sample) => sample.at >= cutoff) ?? samples[0];
  const elapsed = (latest.at - first.at) / 1000;
  const delta = latest.done - first.done;
  if (elapsed < 0.2 || delta < 0 || !Number.isFinite(delta)) return { rate: 0, remaining: null };
  const rate = delta / elapsed;
  if (!Number.isFinite(rate) || rate <= 0) return { rate: 0, remaining: null };
  if (!Number.isFinite(total) || total <= latest.done) return { rate, remaining: null };
  return { rate, remaining: (total - latest.done) / rate };
}

export function formatRate(bytesPerSecond: number, language: string): string {
  return `${formatBytes(bytesPerSecond, language)}/s`;
}

export function formatRemaining(seconds: number, language: string): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  const whole = Math.ceil(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const secs = whole % 60;
  const padded = new Intl.NumberFormat(language, { minimumIntegerDigits: 2, maximumFractionDigits: 0 });
  const plain = new Intl.NumberFormat(language, { maximumFractionDigits: 0 });
  if (hours > 0) return `${plain.format(hours)}:${padded.format(minutes)}:${padded.format(secs)}`;
  return `${plain.format(minutes)}:${padded.format(secs)}`;
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
