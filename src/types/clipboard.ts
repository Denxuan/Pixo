export type ClipKind = "text" | "image" | "files";
export type ClipFilter = "all" | "text" | "image" | "files" | "pinned";

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
  excludedApps: string[];
  skipSensitive: boolean;
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
  excludedApps: string[];
  skipSensitive: boolean;
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
  reason: string | null;
  storagePath: string;
  access: string | null;
}

export interface CopyResult {
  missing: string[];
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
    excludedApps: settings.excludedApps,
    skipSensitive: settings.skipSensitive,
    confirm,
  };
}
