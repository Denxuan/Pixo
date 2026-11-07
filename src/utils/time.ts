const BYTE_UNITS = ["byte", "kilobyte", "megabyte", "gigabyte"] as const;

export function formatBytes(bytes: number, language: string): string {
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < BYTE_UNITS.length - 1) {
    value /= 1024;
    index += 1;
  }
  return new Intl.NumberFormat(language, {
    style: "unit",
    unit: BYTE_UNITS[index],
    maximumFractionDigits: index === 0 ? 0 : 1,
  }).format(value);
}

export function formatRelativeTime(timestamp: number, language: string, now = Date.now()): string {
  const formatter = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  const delta = timestamp - now;
  const seconds = Math.round(delta / 1000);
  const minutes = Math.round(delta / 60_000);
  const hours = Math.round(delta / 3_600_000);
  const days = Math.round(delta / 86_400_000);
  if (Math.abs(seconds) < 60) return formatter.format(seconds, "second");
  if (Math.abs(minutes) < 60) return formatter.format(minutes, "minute");
  if (Math.abs(hours) < 24) return formatter.format(hours, "hour");
  return formatter.format(days, "day");
}

export function bytesToMegabytes(bytes: number): string {
  return String(Math.round((bytes / (1024 * 1024)) * 1000) / 1000);
}

export function megabytesToBytes(value: string): number {
  return Math.round(Number(value) * 1024 * 1024);
}
