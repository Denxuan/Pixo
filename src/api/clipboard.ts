import { invoke } from "@tauri-apps/api/core";

import type {
  ClipDetail,
  ClipKind,
  ClipListPage,
  ClipboardSettings,
  ClipboardSettingsUpdate,
  ClipboardStatus,
  CopyResult,
  ClipSummary,
} from "@/types/clipboard";
import { isDesktop } from "@/utils/desktop";

export class ClipboardRequestError extends Error {
  readonly code: string;
  readonly params: Record<string, unknown>;

  constructor(code: string, params: Record<string, unknown> = {}) {
    super(code);
    this.name = "ClipboardRequestError";
    this.code = code;
    this.params = params;
  }
}

function assertDesktop() {
  if (!isDesktop()) throw new ClipboardRequestError("desktop_only");
}

export function toClipboardError(error: unknown): ClipboardRequestError {
  if (error instanceof ClipboardRequestError) return error;
  if (typeof error === "string") {
    try {
      return toClipboardError(JSON.parse(error));
    } catch {
      return new ClipboardRequestError("unknown");
    }
  }
  if (error && typeof error === "object" && "code" in error) {
    const record = error as { code?: unknown; params?: unknown };
    const params = record.params && typeof record.params === "object" ? record.params as Record<string, unknown> : {};
    return new ClipboardRequestError(String(record.code), params);
  }
  return new ClipboardRequestError("unknown");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  assertDesktop();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toClipboardError(error);
  }
}

export function clipboardList(input: {
  query?: string;
  kind?: ClipKind;
  pinnedOnly?: boolean;
  cursor?: string;
  limit?: number;
}): Promise<ClipListPage> {
  return call("clipboard_list", input);
}

export function clipboardGet(id: number): Promise<ClipDetail> {
  return call("clipboard_get", { id });
}

function toBytes(value: unknown): Uint8Array {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value) && value.every((item) => typeof item === "number")) {
    return Uint8Array.from(value);
  }
  throw new ClipboardRequestError("unknown");
}

export async function clipboardImage(id: number, thumb: boolean): Promise<Uint8Array> {
  return toBytes(await call<unknown>("clipboard_image", { id, thumb }));
}

export async function clipboardSourceIcon(source: string): Promise<{ name: string; iconPng: Uint8Array } | null> {
  const value = await call<{ name?: unknown; iconPng?: unknown } | null>("clipboard_source_icon", { source });
  if (!value || typeof value.name !== "string" || value.name.length === 0) return null;
  return { name: value.name, iconPng: toBytes(value.iconPng ?? []) };
}

export function clipboardCopy(id: number): Promise<CopyResult> {
  return call("clipboard_copy", { id });
}

export function clipboardSetPinned(id: number, pinned: boolean): Promise<void> {
  return call("clipboard_set_pinned", { id, pinned });
}

export function clipboardDelete(ids: number[]): Promise<void> {
  return call("clipboard_delete", { ids });
}

export function clipboardClear(keepPinned: boolean): Promise<void> {
  return call("clipboard_clear", { keepPinned });
}

export function clipboardReadNow(): Promise<ClipSummary> {
  return call("clipboard_read_now");
}

export function clipboardStatus(): Promise<ClipboardStatus> {
  return call("clipboard_status");
}

export function clipboardSetPaused(paused: boolean): Promise<ClipboardStatus> {
  return call("clipboard_set_paused", { paused });
}

export function clipboardGetSettings(): Promise<ClipboardSettings> {
  return call("clipboard_get_settings");
}

export function clipboardUpdateSettings(settings: ClipboardSettingsUpdate): Promise<ClipboardSettings> {
  return call("clipboard_update_settings", { settings });
}
