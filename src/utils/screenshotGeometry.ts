import type { EditorDocument, Mark, Point, Rect, StrokeSize, Watermark } from "@/types/screenshot";

export const CLICK_THRESHOLD = 4;
export const TOOLBAR_HEIGHT = 40;
export const PROPERTY_HEIGHT = 36;
export const MAGNIFIER_SIZE = 128;
// Canvas rejects several CSS generic families and then draws text at the default 10px.
export const CANVAS_FONT_FAMILY = '-apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif';

export const PALETTE = {
  red: "#E5484D",
  yellow: "#F5C518",
  green: "#30A46C",
  blue: "#3B82F6",
  black: "#1C1C1C",
  white: "#FFFFFF",
} as const;

const STROKE: Record<StrokeSize, number> = { sm: 2, md: 4, lg: 8 };
const TEXT_SIZE: Record<StrokeSize, number> = { sm: 14, md: 22, lg: 32 };
const MOSAIC: Record<StrokeSize, number> = { sm: 8, md: 16, lg: 24 };

export type Box = { x: number; y: number; w: number; h: number };
export type ToolbarSide = "below" | "above" | "inside";
export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export type Block = { x: number; y: number };

export function strokeWidth(size: StrokeSize) {
  return STROKE[size];
}

export function textSize(size: StrokeSize) {
  return TEXT_SIZE[size];
}

export function canvasFont(size: number) {
  const px = Number.isFinite(size) && size > 0 ? Math.min(size, 200) : 36;
  return `${px}px ${CANVAS_FONT_FAMILY}`;
}

export function keepTextSize(previous: EditorDocument, received: EditorDocument): EditorDocument {
  const fonts = previous.marks.flatMap((mark) => (mark.kind === "text" && mark.fontSize ? [mark.fontSize] : []));
  let cursor = 0;
  let changed = false;
  const marks = received.marks.map((mark) => {
    if (mark.kind !== "text") return mark;
    const fallback = fonts[cursor] || 36;
    cursor += 1;
    if (mark.fontSize) return mark;
    changed = true;
    return { ...mark, fontSize: fallback };
  });
  return changed ? { ...received, marks } : received;
}

export function sameInkMark(left: Mark, right: Mark) {
  if (left.kind !== right.kind) return false;
  if (left.kind === "text" && right.kind === "text") {
    return Math.abs(left.x - right.x) < 0.5
      && Math.abs(left.y - right.y) < 0.5
      && Math.abs(left.w - right.w) < 0.5
      && left.text === right.text
      && left.color === right.color
      && Math.abs(left.fontSize - right.fontSize) < 0.5;
  }
  if (left.kind === "tag" && right.kind === "tag") {
    return Math.abs(left.anchorX - right.anchorX) < 0.5
      && Math.abs(left.anchorY - right.anchorY) < 0.5
      && left.text === right.text
      && left.size === right.size
      && left.color === right.color
      && left.side === right.side;
  }
  return false;
}

export function keepsLocalInk(local: EditorDocument, incoming: EditorDocument) {
  return local.marks.some((mark) => (mark.kind === "text" || mark.kind === "tag") && !incoming.marks.some((item) => sameInkMark(item, mark)));
}

export function mergeInkMarks(document: EditorDocument, extra: readonly Mark[]) {
  let marks = document.marks;
  for (const mark of extra) {
    if (mark.kind !== "text" && mark.kind !== "tag") continue;
    if (marks.some((item) => sameInkMark(item, mark))) continue;
    if (marks.length >= 300) break;
    marks = [...marks, mark];
  }
  return marks === document.marks ? document : { ...document, marks };
}

export function mosaicBlock(size: StrokeSize) {
  return MOSAIC[size];
}

export function isClick(dx: number, dy: number) {
  return Math.hypot(dx, dy) < CLICK_THRESHOLD;
}

export function imagePoint(
  clientX: number,
  clientY: number,
  rect: { left: number; top: number; width: number; height: number },
  imageWidth: number,
  imageHeight: number,
): Point {
  return {
    x: ((clientX - rect.left) / rect.width) * imageWidth,
    y: ((clientY - rect.top) / rect.height) * imageHeight,
  };
}

export function clampSelection(rect: Rect, width: number, height: number): Rect | null {
  const left = Math.max(0, Math.min(rect.x, rect.x + rect.w));
  const top = Math.max(0, Math.min(rect.y, rect.y + rect.h));
  const right = Math.max(0, Math.min(width, Math.max(rect.x, rect.x + rect.w)));
  const bottom = Math.max(0, Math.min(height, Math.max(rect.y, rect.y + rect.h)));
  const w = right - left;
  const h = bottom - top;
  if (w < 1 || h < 1) return null;
  return { x: left, y: top, w, h };
}

export function moveSelection(rect: Rect, dx: number, dy: number, width: number, height: number): Rect {
  return {
    ...rect,
    x: Math.min(Math.max(0, rect.x + dx), Math.max(0, width - rect.w)),
    y: Math.min(Math.max(0, rect.y + dy), Math.max(0, height - rect.h)),
  };
}

export function resizeSelection(origin: Rect, handle: Handle, x: number, y: number, width: number, height: number): Rect | null {
  let left = origin.x;
  let top = origin.y;
  let right = origin.x + origin.w;
  let bottom = origin.y + origin.h;
  if (handle.includes("w")) left = x;
  if (handle.includes("e")) right = x;
  if (handle.includes("n")) top = y;
  if (handle.includes("s")) bottom = y;
  return clampSelection({ x: left, y: top, w: right - left, h: bottom - top }, width, height);
}

export function hitHandle(x: number, y: number, rect: Rect, scale: number): Handle | null {
  const reach = 4 * Math.max(scale, 1);
  const points = handlePoints(rect);
  for (const point of points) {
    if (Math.abs(point.x - x) <= reach && Math.abs(point.y - y) <= reach) return point.id;
  }
  return null;
}

export function handlePoints(rect: Rect): Array<{ id: Handle; x: number; y: number }> {
  const right = rect.x + rect.w;
  const bottom = rect.y + rect.h;
  const midX = rect.x + rect.w / 2;
  const midY = rect.y + rect.h / 2;
  return [
    { id: "nw", x: rect.x, y: rect.y },
    { id: "n", x: midX, y: rect.y },
    { id: "ne", x: right, y: rect.y },
    { id: "e", x: right, y: midY },
    { id: "se", x: right, y: bottom },
    { id: "s", x: midX, y: bottom },
    { id: "sw", x: rect.x, y: bottom },
    { id: "w", x: rect.x, y: midY },
  ];
}

export function contains(rect: Rect, x: number, y: number) {
  return x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h;
}

export function topWindow(x: number, y: number, windows: Rect[]): Rect | null {
  for (const window of windows) {
    if (contains(window, x, y)) return window;
  }
  return null;
}

export function windowLocal(window: { x: number; y: number; width: number; height: number }, monitor: { x: number; y: number }): Rect {
  return { x: window.x - monitor.x, y: window.y - monitor.y, w: window.width, h: window.height };
}

export function placeToolbar(selection: Box, toolbar: { w: number; h: number }, workspace: Box, gap = 8): { x: number; y: number; side: ToolbarSide } {
  const minX = workspace.x;
  const maxX = Math.max(minX, workspace.x + workspace.w - toolbar.w);
  const x = Math.min(Math.max(selection.x + selection.w / 2 - toolbar.w / 2, minX), maxX);
  const below = selection.y + selection.h + gap;
  if (below + toolbar.h <= workspace.y + workspace.h) return { x, y: below, side: "below" };
  const above = selection.y - gap - toolbar.h;
  if (above >= workspace.y) return { x, y: above, side: "above" };
  const inside = Math.min(selection.y + selection.h - toolbar.h, workspace.y + workspace.h - toolbar.h);
  return { x, y: Math.max(workspace.y, inside), side: "inside" };
}

export function placePropertyBar(
  toolbar: { x: number; y: number; h: number },
  bar: { h: number },
  side: ToolbarSide,
  gap = 6,
): { x: number; y: number } {
  if (side === "above") return { x: toolbar.x, y: toolbar.y + toolbar.h + gap };
  return { x: toolbar.x, y: toolbar.y - gap - bar.h };
}

export function placeMagnifier(pointer: Point, bounds: Box, box: { w: number; h: number } = { w: MAGNIFIER_SIZE, h: MAGNIFIER_SIZE }, offset = 16): Point {
  const right = pointer.x + offset;
  const below = pointer.y + offset;
  if (right + box.w <= bounds.x + bounds.w && below + box.h <= bounds.y + bounds.h) return { x: right, y: below };
  return { x: pointer.x - offset - box.w, y: pointer.y - offset - box.h };
}

export function placeSizeLabel(selection: Box, label: { w: number; h: number }, workspace: Box, gap = 8): Point {
  const x = selection.x + selection.w + gap;
  const below = selection.y + selection.h + gap;
  const overflows = below + label.h > workspace.y + workspace.h || x + label.w > workspace.x + workspace.w;
  return { x, y: overflows ? selection.y - gap - label.h : below };
}

export function textBoxWidth(x: number, selection: Rect) {
  const room = selection.x + selection.w - x;
  if (room >= 80) return Math.min(240, room);
  return 80;
}

export function fittedBox(start: Point, end: Point, width: number, height: number, minW: number, minH: number): Rect | null {
  let x = Math.min(start.x, end.x);
  let y = Math.min(start.y, end.y);
  let w = Math.max(minW, Math.abs(end.x - start.x));
  let h = Math.max(minH, Math.abs(end.y - start.y));
  if (x + w > width) x = Math.max(0, width - w);
  if (y + h > height) y = Math.max(0, height - h);
  w = Math.min(w, Math.max(0, width - x));
  h = Math.min(h, Math.max(0, height - y));
  if (w < 1 || h < 1) return null;
  return { x, y, w, h };
}

export function resizeTextBox(origin: Rect, handle: Handle, point: Point, width: number, height: number, minW: number, minH: number): Rect | null {
  const resized = resizeSelection(origin, handle, point.x, point.y, width, height);
  if (!resized) return null;
  let { x, y, w, h } = resized;
  if (w < minW) {
    if (handle.includes("w")) x -= minW - w;
    w = minW;
  }
  if (h < minH) {
    if (handle.includes("n")) y -= minH - h;
    h = minH;
  }
  if (x < 0) {
    w += x;
    x = 0;
  }
  if (y < 0) {
    h += y;
    y = 0;
  }
  if (x + w > width) w = width - x;
  if (y + h > height) h = height - y;
  if (w < 1 || h < 1) return null;
  return { x, y, w, h };
}

export function watermarkPayload(value: Watermark): Watermark | null {
  const text = value.text.trim().slice(0, 40);
  if (!text) return null;
  const opacity = Math.min(1, Math.max(0.1, Math.round(value.opacity * 10) / 10));
  return { text, opacity, color: value.color };
}

export function blockOrigin(value: number, block: number) {
  return Math.floor(value / block) * block;
}

export function blocksForRect(rect: Rect, block: number, imageWidth: number, imageHeight: number): Block[] {
  if (block <= 0) return [];
  const x0 = Math.floor(rect.x / block);
  const y0 = Math.floor(rect.y / block);
  const x1 = Math.floor((rect.x + Math.max(rect.w, 0) - 1e-6) / block);
  const y1 = Math.floor((rect.y + Math.max(rect.h, 0) - 1e-6) / block);
  const cells: Block[] = [];
  for (let y = y0; y <= y1; y += 1) {
    for (let x = x0; x <= x1; x += 1) {
      const originX = x * block;
      const originY = y * block;
      if (originX < imageWidth && originY < imageHeight && originX + block > 0 && originY + block > 0) {
        cells.push({ x: originX, y: originY });
      }
    }
  }
  return cells;
}

export function blocksForStroke(points: Point[], block: number, imageWidth: number, imageHeight: number): Block[] {
  if (points.length === 0 || block <= 0) return [];
  const cells: Block[] = [];
  const seen = new Set<string>();
  const add = (rect: Rect) => {
    for (const cell of blocksForRect(rect, block, imageWidth, imageHeight)) {
      const key = `${cell.x},${cell.y}`;
      if (seen.has(key)) continue;
      seen.add(key);
      cells.push(cell);
    }
  };
  for (let index = 0; index < points.length; index += 1) {
    const current = points[index];
    const next = points[Math.min(index + 1, points.length - 1)];
    const steps = Math.max(1, Math.ceil(Math.hypot(next.x - current.x, next.y - current.y)));
    for (let step = 0; step <= steps; step += 1) {
      const t = step / steps;
      const x = current.x + (next.x - current.x) * t;
      const y = current.y + (next.y - current.y) * t;
      add({ x: x - block / 2, y: y - block / 2, w: block, h: block });
    }
  }
  return cells;
}

export function dedupeBlocks(cells: Block[]): Block[] {
  const seen = new Set<string>();
  const unique: Block[] = [];
  for (const cell of cells) {
    const key = `${cell.x},${cell.y}`;
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(cell);
  }
  return unique;
}

export function applyMosaic(
  base: Uint8ClampedArray,
  sample: Uint8ClampedArray,
  width: number,
  height: number,
  cells: Block[],
  block: number,
) {
  const out = new Uint8ClampedArray(base);
  for (const cell of dedupeBlocks(cells)) {
    const x0 = Math.max(0, cell.x);
    const y0 = Math.max(0, cell.y);
    const x1 = Math.min(width, cell.x + block);
    const y1 = Math.min(height, cell.y + block);
    if (x1 <= x0 || y1 <= y0) continue;
    let red = 0;
    let green = 0;
    let blue = 0;
    let alpha = 0;
    let count = 0;
    for (let y = y0; y < y1; y += 1) {
      for (let x = x0; x < x1; x += 1) {
        const index = (y * width + x) * 4;
        red += sample[index];
        green += sample[index + 1];
        blue += sample[index + 2];
        alpha += sample[index + 3];
        count += 1;
      }
    }
    const next = [Math.round(red / count), Math.round(green / count), Math.round(blue / count), Math.round(alpha / count)];
    for (let y = y0; y < y1; y += 1) {
      for (let x = x0; x < x1; x += 1) {
        out.set(next, (y * width + x) * 4);
      }
    }
  }
  return out;
}

export function arrowHead(x1: number, y1: number, x2: number, y2: number, stroke: number) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const length = Math.hypot(dx, dy);
  const head = Math.min(stroke * 4, length);
  const ux = length === 0 ? 1 : dx / length;
  const uy = length === 0 ? 0 : dy / length;
  const spread = Math.tan((26 * Math.PI) / 180 / 2) * head;
  const baseX = x2 - ux * head;
  const baseY = y2 - uy * head;
  return {
    tip: { x: x2, y: y2 },
    shaft: { x: baseX, y: baseY },
    left: { x: baseX + -uy * spread, y: baseY + ux * spread },
    right: { x: baseX - -uy * spread, y: baseY - ux * spread },
  };
}

export function tagBubble(anchorX: number, anchorY: number, textWidth: number, textHeight: number, side: "left" | "right") {
  const w = textWidth + 16;
  const h = textHeight + 16;
  return {
    x: side === "right" ? anchorX + 12 : anchorX - 12 - w,
    y: anchorY - h / 2,
    w,
    h,
  };
}

export function parseFrame(bytes: Uint8Array) {
  if (bytes.length < 16) throw new Error("capture_failed");
  if (bytes[0] !== 80 || bytes[1] !== 88 || bytes[2] !== 70 || bytes[3] !== 82) throw new Error("capture_failed");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const width = view.getUint32(4, true);
  const height = view.getUint32(8, true);
  const stride = view.getUint32(12, true);
  if (width === 0 || height === 0 || stride !== width * 4 || bytes.length !== 16 + stride * height) {
    throw new Error("capture_failed");
  }
  return {
    width,
    height,
    pixels: new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 16, stride * height),
  };
}

export function shotMonitorIndex(label: string, search: string) {
  const monitor = new URLSearchParams(search).get("monitor");
  if (monitor != null && monitor !== "") {
    const index = Number(monitor);
    if (Number.isInteger(index) && index >= 0 && index < 8) return index;
  }
  return overlayIndex(label);
}

export function overlayIndex(label: string) {
  const match = /^screenshot-overlay-(\d+)$/.exec(label);
  if (!match) return null;
  const slot = Number(match[1]);
  return slot >= 1 && slot <= 8 ? slot - 1 : null;
}

export function draggedOrigin(start: Point, origin: Point, pointer: Point): Point {
  return { x: start.x + pointer.x - origin.x, y: start.y + pointer.y - origin.y };
}

export function pinSlot(label: string) {
  const match = /^screenshot-pin-(\d+)$/.exec(label);
  if (!match) return null;
  const slot = Number(match[1]);
  return slot >= 1 && slot <= 8 ? slot : null;
}

export function exportBounds(selection: Rect, imageWidth: number, imageHeight: number) {
  const x = Math.max(0, Math.min(imageWidth - 1, Math.round(selection.x)));
  const y = Math.max(0, Math.min(imageHeight - 1, Math.round(selection.y)));
  const w = Math.max(1, Math.min(Math.round(selection.w), imageWidth - x));
  const h = Math.max(1, Math.min(Math.round(selection.h), imageHeight - y));
  return { x, y, w, h };
}

export function clampPoint(point: Point, selection: Rect): Point {
  return {
    x: Math.min(Math.max(selection.x, point.x), selection.x + selection.w),
    y: Math.min(Math.max(selection.y, point.y), selection.y + selection.h),
  };
}

export function pixelHex(pixels: Uint8ClampedArray, width: number, height: number, x: number, y: number) {
  const sx = Math.round(x);
  const sy = Math.round(y);
  if (sx < 0 || sy < 0 || sx >= width || sy >= height) return "#000000";
  const index = (sy * width + sx) * 4;
  return `#${toHex(pixels[index])}${toHex(pixels[index + 1])}${toHex(pixels[index + 2])}`;
}

function toHex(value: number) {
  return value.toString(16).toUpperCase().padStart(2, "0");
}
