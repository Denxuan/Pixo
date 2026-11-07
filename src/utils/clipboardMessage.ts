import type { ClipboardRequestError } from "@/api/clipboard";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function clipboardErrorText(t: Translate, error: ClipboardRequestError): string {
  if (error.code === "invalid_input" && error.params.reason === "kind_disabled") {
    return t("zdx.text.clipboard.kindDisabled");
  }
  if (error.code === "needs_confirm") {
    return t("zdx.text.clipboard.confirmPrune", { count: Number(error.params.count ?? 0) });
  }
  switch (error.code) {
    case "unsupported":
      return t("zdx.text.clipboard.error.unsupported");
    case "permission_denied":
      return t("zdx.text.clipboard.error.permission_denied");
    case "busy":
      return t("zdx.text.clipboard.error.busy");
    case "not_found":
      return t("zdx.text.clipboard.error.not_found");
    case "too_large":
      return t("zdx.text.clipboard.error.too_large");
    case "file_missing":
      return t("zdx.text.clipboard.error.file_missing");
    case "storage":
      return t("zdx.text.clipboard.error.storage");
    case "invalid_input":
      return t("zdx.text.clipboard.error.invalid_input");
    case "shortcut_conflict":
      return t("zdx.text.clipboard.error.shortcut_conflict");
    case "desktop_only":
      return t("zdx.text.clipboard.error.desktop_only");
    default:
      return t("zdx.text.clipboard.error.unknown");
  }
}

export function clipboardStatusText(t: Translate, state: string): string {
  switch (state) {
    case "running":
      return t("zdx.text.clipboard.running");
    case "paused":
      return t("zdx.text.clipboard.paused");
    case "disabled":
      return t("zdx.text.clipboard.disabled");
    case "permission_denied":
      return t("zdx.text.clipboard.permissionDenied");
    case "unsupported":
      return t("zdx.text.clipboard.unsupported");
    default:
      return t("zdx.text.clipboard.unsupported");
  }
}

export function clipboardReasonText(t: Translate, reason: string | null): string {
  switch (reason) {
    case "wayland_no_data_control":
      return t("zdx.text.clipboard.statusReason.wayland_no_data_control");
    case "no_display":
      return t("zdx.text.clipboard.statusReason.no_display");
    case "platform":
    case "x11":
      return t("zdx.text.clipboard.statusReason.platform");
    default:
      return "";
  }
}
