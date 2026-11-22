import type { DockerRequestError } from "@/api/docker";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function dockerErrorText(t: Translate, error: Pick<DockerRequestError, "code" | "params">): string {
  const key = `zdx.text.docker.error.${error.code}`;
  const translated = t(key, error.params);
  return translated === key ? t("zdx.text.docker.error.unknown") : translated;
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  const digits = size >= 10 || index === 0 ? 0 : 1;
  return `${size.toFixed(digits)} ${units[index]}`;
}

export function formatTime(seconds: number): string {
  if (!seconds) return "";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(seconds * 1000);
}
