import type { ScreenshotRequestError } from "@/api/screenshot";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function screenshotErrorText(t: Translate, error: ScreenshotRequestError) {
  const reason = error.params.reason;
  if (error.code === "permission_denied" && reason === "accessibility") return t("zdx.text.screenshot.scrollNeedsAccess");
  if (error.code === "invalid_input" && reason === "scroll") return t("zdx.text.screenshot.scrollNeedSelection");
  if (error.code === "not_found" && reason === "scroll") return t("zdx.text.screenshot.scrollStill");
  if (error.code === "unsupported" && reason === "wayland") return t("zdx.text.screenshot.wayland");
  if (error.code === "capture_failed" && reason === "display_changed") return t("zdx.text.screenshot.displayChanged");
  if (error.code === "capture_failed" && reason === "empty") return t("zdx.text.screenshot.recordEmpty");
  if (error.code === "capture_failed" && reason === "audio") return t("zdx.text.screenshot.audioUnavailable");
  if (error.code === "unsupported" && reason === "ocr") return t("zdx.text.screenshot.ocrUnsupported");
  if (error.code === "not_found" && reason === "ocr") return t("zdx.text.screenshot.ocrEmpty");
  switch (error.code) {
    case "unsupported":
      return t("zdx.text.screenshot.error.unsupported");
    case "permission_denied":
      return t("zdx.text.screenshot.error.permission_denied");
    case "capture_failed":
      return t("zdx.text.screenshot.error.capture_failed");
    case "busy":
      return t("zdx.text.screenshot.error.busy");
    case "not_found":
      return t("zdx.text.screenshot.error.not_found");
    case "storage":
      return t("zdx.text.screenshot.error.storage");
    case "invalid_input":
      return t("zdx.text.screenshot.error.invalid_input");
    case "shortcut_conflict":
      return t("zdx.text.screenshot.error.shortcut_conflict");
    case "pin_limit":
      return t("zdx.text.screenshot.error.pin_limit");
    case "cancelled":
      return t("zdx.text.screenshot.error.cancelled");
    case "desktop_only":
      return t("zdx.text.screenshot.error.desktop_only");
    case "too_large":
      return t("zdx.text.screenshot.error.too_large");
    default:
      return t("zdx.text.screenshot.error.unknown");
  }
}

export function screenshotReasonText(t: Translate, reason: string | null) {
  if (reason === "wayland") return t("zdx.text.screenshot.wayland");
  if (reason === "permission_denied") return t("zdx.text.screenshot.permission");
  return "";
}
