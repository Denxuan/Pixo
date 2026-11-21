import { invoke } from "@tauri-apps/api/core";

import type { CopyResult, EditorDocument, SaveResult, SessionView, ShotSettings, ShotStatus } from "@/types/screenshot";
import { isDesktop } from "@/utils/desktop";

export class ScreenshotRequestError extends Error {
  readonly code: string;
  readonly params: Record<string, unknown>;

  constructor(code: string, params: Record<string, unknown> = {}) {
    super(code);
    this.name = "ScreenshotRequestError";
    this.code = code;
    this.params = params;
  }
}

function assertDesktop() {
  if (!isDesktop()) throw new ScreenshotRequestError("desktop_only");
}

export function toScreenshotError(error: unknown): ScreenshotRequestError {
  if (error instanceof ScreenshotRequestError) return error;
  if (typeof error === "string") {
    try {
      return toScreenshotError(JSON.parse(error));
    } catch {
      return new ScreenshotRequestError("unknown");
    }
  }
  if (error && typeof error === "object" && "code" in error) {
    const record = error as { code?: unknown; params?: unknown };
    const params = record.params && typeof record.params === "object" ? record.params as Record<string, unknown> : {};
    return new ScreenshotRequestError(String(record.code), params);
  }
  return new ScreenshotRequestError("unknown");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  assertDesktop();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toScreenshotError(error);
  }
}

function toBytes(value: unknown): Uint8Array {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  if (Array.isArray(value) && value.every((item) => typeof item === "number")) return Uint8Array.from(value);
  throw new ScreenshotRequestError("unknown");
}

export function screenshotStatus() {
  return call<ShotStatus>("screenshot_status");
}

export function screenshotGetSettings() {
  return call<ShotSettings>("screenshot_get_settings");
}

export function screenshotUpdateSettings(settings: ShotSettings) {
  return call<ShotSettings>("screenshot_update_settings", { settings });
}

export function screenshotSetEnabled(enabled: boolean) {
  return call<ShotSettings>("screenshot_set_enabled", { enabled });
}

export function screenshotPickSaveDir(title: string) {
  return call<string | null>("screenshot_pick_save_dir", { title });
}

export function screenshotRequestAccess() {
  return call<ShotStatus>("screenshot_request_access");
}

export function screenshotStart(hideSelf?: boolean) {
  return call<SessionView>("screenshot_start", { mode: "capture", hideSelf });
}

export function screenshotCurrent() {
  return call<SessionView>("screenshot_current");
}

export function screenshotOverlayReady(sessionId: number) {
  return call<void>("screenshot_overlay_ready", { sessionId });
}

export async function screenshotFrame(index: number) {
  return toBytes(await call<unknown>("screenshot_frame", { index }));
}

export function screenshotEditorCommit(baseVersion: number, document: EditorDocument) {
  return call<SessionView>("screenshot_editor_commit", { baseVersion, document });
}

export function screenshotEditorUndo() {
  return call<SessionView>("screenshot_editor_undo");
}

export function screenshotEditorRedo() {
  return call<SessionView>("screenshot_editor_redo");
}

export function screenshotCancel() {
  return call<void>("screenshot_cancel");
}

export function screenshotFinish() {
  return call<void>("screenshot_finish");
}

export type AudioDevices = {
  speakers: string[];
  microphones: string[];
};

export function screenshotRecordStart(monitorIndex: number, x: number, y: number, width: number, height: number, format: "gif" | "mp4", cursor: "off" | "highlight" | "click" | "both", speaker: boolean, microphone: boolean, speakerDevice: string, microphoneDevice: string) {
  return call<void>("screenshot_record_start", { monitorIndex, x, y, width, height, format, cursor, speaker, microphone, speakerDevice, microphoneDevice });
}

export function screenshotAudioDevices() {
  return call<AudioDevices>("screenshot_audio_devices");
}

export function screenshotCameraOpen(monitorIndex: number, x: number, y: number, width: number, height: number, deviceId: string) {
  return call<boolean>("screenshot_camera_open", { monitorIndex, x, y, width, height, deviceId });
}

export function screenshotCameraClose() {
  return call<void>("screenshot_camera_close");
}

export function screenshotRecordStop() {
  return call<void>("screenshot_record_stop");
}

export async function screenshotRecordPreview() {
  return toBytes(await call<unknown>("screenshot_record_preview"));
}

export function screenshotRecordDownload() {
  return call<SaveResult>("screenshot_record_download");
}

export function screenshotRecordCopy() {
  return call<void>("screenshot_record_copy");
}

export function screenshotRecordClose() {
  return call<void>("screenshot_record_close");
}

export function screenshotRecognizeText(pngBase64: string) {
  return call<{ text: string }>("screenshot_recognize_text", { pngBase64 });
}

export function screenshotCopyText(text: string) {
  return call<void>("screenshot_copy_text", { text });
}

export function screenshotScroll(monitorIndex: number, x: number, y: number, width: number, height: number) {
  return call<void>("screenshot_scroll", { monitorIndex, x, y, width, height });
}

export function screenshotScrollStop() {
  return call<void>("screenshot_scroll_stop");
}

export function screenshotScrollSetAuto(enabled: boolean) {
  return call<ShotSettings>("screenshot_scroll_set_auto", { enabled });
}

export function screenshotScrollCopy() {
  return call<CopyResult>("screenshot_scroll_copy");
}

export function screenshotScrollSave() {
  return call<SaveResult>("screenshot_scroll_save");
}

export function screenshotCopy(pngBase64: string) {
  return call<CopyResult>("screenshot_copy", { pngBase64 });
}

export function screenshotSave(pngBase64: string) {
  return call<SaveResult>("screenshot_save", { pngBase64 });
}

export function screenshotAdoptPin(x: number, y: number, width: number, height: number, scaleFactor: number, cropX: number, cropY: number, pageWidth: number, pageHeight: number) {
  return call<boolean>("screenshot_adopt_pin", { x, y, width, height, scaleFactor, cropX, cropY, pageWidth, pageHeight });
}

export function screenshotReleasePin() {
  return call<void>("screenshot_release_pin");
}

export function screenshotPin(pngBase64: string, x: number, y: number, width: number, height: number, scaleFactor: number) {
  return call<number>("screenshot_pin", { pngBase64, x, y, width, height, scaleFactor });
}

export function screenshotPinReady(slot: number) {
  return call<void>("screenshot_pin_ready", { slot });
}

export async function screenshotPinImage(slot: number) {
  return toBytes(await call<unknown>("screenshot_pin_image", { slot }));
}

export function screenshotPinCopy(slot: number) {
  return call<CopyResult>("screenshot_pin_copy", { slot });
}

export function screenshotPinClose(slot: number) {
  return call<void>("screenshot_pin_close", { slot });
}

export function openScreenRecordingSettings() {
  if (!isDesktop()) return Promise.resolve();
  return invoke("permissions_open", { id: "screenRecording" });
}
