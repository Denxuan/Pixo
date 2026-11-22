export type ClipKind = "text" | "image" | "files";
export type ClipFilter = "all" | "text" | "image" | "files" | "pinned";

export const CLIP_FILTERS: Array<{ id: ClipFilter; label: string }> = [
  { id: "all", label: "zdx.label.clipboard.filter.all" },
  { id: "text", label: "zdx.label.clipboard.filter.text" },
  { id: "image", label: "zdx.label.clipboard.filter.image" },
  { id: "files", label: "zdx.label.clipboard.filter.files" },
  { id: "pinned", label: "zdx.label.clipboard.filter.pinned" },
];

export function isClipFilter(value: unknown): value is ClipFilter {
  return value === "all" || value === "text" || value === "image" || value === "files" || value === "pinned";
}

export interface ClipSummary {
  id: number;
  kind: string;
  preview: string;
  fileNames: string[];
  imageWidth: number | null;
  imageHeight: number | null;
  sizeBytes: number;
  sourceApp: string | null;
  pinned: boolean;
  createdAt: number;
  lastCopiedAt: number;
  copyCount: number;
  formatted: boolean;
  pinKey: string | null;
  label: string | null;
}

export interface ClipDetail {
  id: number;
  kind: string;
  text: string | null;
  files: string[];
  imageWidth: number | null;
  imageHeight: number | null;
  sizeBytes: number;
  sourceApp: string | null;
  pinned: boolean;
  createdAt: number;
  lastCopiedAt: number;
  copyCount: number;
  label: string | null;
}

export interface ClipListPage {
  items: ClipSummary[];
  nextCursor: string | null;
}

export interface ClipboardSettings {
  enabled: boolean;
  paused: boolean;
  recordText: boolean;
  recordImage: boolean;
  recordFiles: boolean;
  maxItems: number;
  retentionDays: number;
  maxTextBytes: number;
  maxImageBytes: number;
  shortcut: string;
  pinShortcut: string;
  deleteShortcut: string;
  excludedApps: string[];
  skipSensitive: boolean;
  copyOnSelect: boolean;
  pasteAutomatically: boolean;
  ignoredTypes: string[];
}

export interface ClipboardSettingsUpdate {
  enabled: boolean;
  recordText: boolean;
  recordImage: boolean;
  recordFiles: boolean;
  maxItems: number;
  retentionDays: number;
  maxTextBytes: number;
  maxImageBytes: number;
  shortcut: string;
  pinShortcut: string;
  deleteShortcut: string;
  excludedApps: string[];
  skipSensitive: boolean;
  copyOnSelect: boolean;
  pasteAutomatically: boolean;
  ignoredTypes: string[];
  confirm?: boolean;
}

export interface ClipboardStatus {
  state: string;
  platform: string;
  canMonitor: boolean;
  canReadSourceApp: boolean;
  canGlobalShortcut: boolean;
  enabled: boolean;
  paused: boolean;
  skippedSensitive: number;
  skippedTooLarge: number;
  skippedExcluded: number;
  skippedIgnored: number;
  ignoreNext: boolean;
  reason: string | null;
  storagePath: string;
  access: string | null;
}

export interface CopyResult {
  missing: string[];
  pasted: boolean;
  pasteError: string | null;
}

export function toSettingsUpdate(settings: ClipboardSettings, confirm = false): ClipboardSettingsUpdate {
  return {
    enabled: settings.enabled,
    recordText: settings.recordText,
    recordImage: settings.recordImage,
    recordFiles: settings.recordFiles,
    maxItems: settings.maxItems,
    retentionDays: settings.retentionDays,
    maxTextBytes: settings.maxTextBytes,
    maxImageBytes: settings.maxImageBytes,
    shortcut: settings.shortcut,
    pinShortcut: settings.pinShortcut,
    deleteShortcut: settings.deleteShortcut,
    excludedApps: settings.excludedApps,
    skipSensitive: settings.skipSensitive,
    copyOnSelect: settings.copyOnSelect,
    pasteAutomatically: settings.pasteAutomatically,
    ignoredTypes: settings.ignoredTypes,
    confirm,
  };
}
