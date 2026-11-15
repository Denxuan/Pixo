export type PaletteColor = "red" | "yellow" | "green" | "blue" | "black" | "white";
export type StrokeSize = "sm" | "md" | "lg";
export type TagSide = "left" | "right";

export type Point = { x: number; y: number };
export type Rect = { x: number; y: number; w: number; h: number };

export type Mark =
  | { kind: "rect"; x: number; y: number; w: number; h: number; size: StrokeSize; color: PaletteColor }
  | { kind: "ellipse"; x: number; y: number; w: number; h: number; size: StrokeSize; color: PaletteColor }
  | { kind: "arrow"; x1: number; y1: number; x2: number; y2: number; size: StrokeSize; color: PaletteColor }
  | { kind: "pen"; points: Point[]; size: StrokeSize; color: PaletteColor }
  | { kind: "highlight"; points: Point[]; size: StrokeSize; color: PaletteColor }
  | { kind: "mosaicPen"; points: Point[]; size: StrokeSize }
  | { kind: "mosaicRect"; x: number; y: number; w: number; h: number; size: StrokeSize }
  | { kind: "text"; x: number; y: number; w: number; text: string; size: StrokeSize; fontSize: number; color: PaletteColor }
  | { kind: "tag"; anchorX: number; anchorY: number; text: string; size: StrokeSize; color: PaletteColor; side: TagSide };

export type Watermark = { text: string; opacity: number; color: PaletteColor };

export type EditorDocument = {
  monitorIndex: number;
  selection: Rect | null;
  marks: Mark[];
  watermark: Watermark | null;
};

export type MonitorInfo = {
  index: number;
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
};

export type SnapInfo = { x: number; y: number; width: number; height: number };

export type SessionView = {
  id: number;
  monitors: MonitorInfo[];
  windows: SnapInfo[];
  document: EditorDocument;
  version: number;
  canUndo: boolean;
  canRedo: boolean;
  truncated: boolean;
  mode: "capture" | "record";
};

export type ShotStatus = {
  platform: string;
  canCapture: boolean;
  canSnap: boolean;
  canScroll: boolean;
  canOcr: boolean;
  canRecord: boolean;
  captureReason: string | null;
  screenRecording: boolean;
  session: string;
  saveDir: string;
};

export type ShotSettings = {
  shortcut: string;
  recordShortcut: string;
  saveDir: string;
  format: "png" | "jpeg";
  jpegQuality: number;
  hideSelf: boolean;
  scrollMaxHeight: number;
  scrollAuto: boolean;
  enabled: boolean;
};

export type CopyResult = { history: string };
export type SaveResult = { path: string };

export type ShotTool = "adjust" | "rect" | "ellipse" | "arrow" | "pen" | "highlight" | "mosaic" | "text" | "tag" | "watermark";
