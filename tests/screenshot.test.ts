import { expect, test } from "vitest";

import {
  applyMosaic,
  arrowHead,
  blockOrigin,
  blocksForRect,
  clampSelection,
  dedupeBlocks,
  exportBounds,
  imagePoint,
  isClick,
  overlayIndex,
  shotMonitorIndex,
  parseFrame,
  draggedOrigin,
  pinSlot,
  placeAttachedProperty,
  placeCameraBubble,
  placeMagnifier,
  placeOuterPropertyBar,
  placePropertyBar,
  placeRecordCard,
  placeSizeLabel,
  placeToolbar,
  tagBubble,
  canvasFont,
  fittedBox,
  keepTextSize,
  keepsLocalInk,
  mergeInkMarks,
  resizeTextBox,
  textBoxWidth,
  watermarkPayload,
} from "@/utils/screenshotGeometry";

test("a short press is a click and four pixels is a drag", () => {
  expect(isClick(3.9, 0)).toBe(true);
  expect(isClick(4, 0)).toBe(false);
  expect(isClick(3, 3)).toBe(false);
});

test("maps a pointer through the canvas box instead of the device ratio", () => {
  expect(imagePoint(100, 50, { left: 0, top: 0, width: 200, height: 100 }, 400, 200)).toEqual({ x: 200, y: 100 });
});

test("clamps a selection inside the frozen image", () => {
  expect(clampSelection({ x: -10, y: 5, w: 30, h: 10 }, 100, 50)).toEqual({ x: 0, y: 5, w: 20, h: 10 });
  expect(clampSelection({ x: 90, y: 40, w: 4, h: 1 }, 100, 40)).toBeNull();
});

test("places the toolbar below, above, and inside the selection", () => {
  const toolbar = { w: 120, h: 40 };
  const workspace = { x: 0, y: 0, w: 1000, h: 800 };
  expect(placeToolbar({ x: 100, y: 100, w: 200, h: 100 }, toolbar, workspace)).toEqual({ x: 140, y: 208, side: "below" });
  expect(placeToolbar({ x: 100, y: 760, w: 200, h: 30 }, toolbar, workspace).side).toBe("above");
  expect(placeToolbar({ x: 10, y: 30, w: 80, h: 40 }, toolbar, { x: 0, y: 0, w: 200, h: 100 }).side).toBe("inside");
});

test("centers the record card in the selection and keeps it inside the workspace", () => {
  expect(placeRecordCard({ x: 100, y: 80, w: 400, h: 200 }, { w: 200, h: 100 }, { x: 0, y: 0, w: 800, h: 600 })).toEqual({ x: 200, y: 130 });
  expect(placeRecordCard({ x: 700, y: 500, w: 80, h: 40 }, { w: 200, h: 120 }, { x: 0, y: 0, w: 800, h: 600 })).toEqual({ x: 600, y: 460 });
  expect(placeRecordCard({ x: 10, y: 10, w: 40, h: 40 }, { w: 200, h: 120 }, { x: 0, y: 0, w: 180, h: 100 })).toEqual({ x: 0, y: 0 });
});

test("puts the property bar on the opposite side of the toolbar", () => {
  expect(placePropertyBar({ x: 10, y: 80, h: 40 }, { h: 36 }, "below")).toEqual({ x: 10, y: 38 });
  expect(placePropertyBar({ x: 10, y: 20, h: 40 }, { h: 36 }, "above")).toEqual({ x: 10, y: 66 });
});

test("puts the camera bubble in the bottom-right of the selection", () => {
  expect(placeCameraBubble({ x: 40, y: 20, w: 500, h: 320 })).toEqual({ x: 356, y: 156, w: 168, h: 168 });
  expect(placeCameraBubble({ x: 0, y: 0, w: 200, h: 180 })).toEqual({ x: 36, y: 16, w: 148, h: 148 });
  expect(placeCameraBubble({ x: 0, y: 0, w: 160, h: 140 })).toEqual({ x: 36, y: 16, w: 108, h: 108 });
  expect(placeCameraBubble({ x: 0, y: 0, w: 90, h: 90 })).toBeNull();
});

test("keeps the attached property bar on the preferred side", () => {
  const workspace = { x: 0, y: 0, w: 1000, h: 800 };
  expect(placeAttachedProperty({ x: 10, y: 80, h: 40 }, { h: 36 }, workspace, "above")).toEqual({ x: 10, y: 38 });
  expect(placeAttachedProperty({ x: 10, y: 80, h: 40 }, { h: 36 }, workspace, "below")).toEqual({ x: 10, y: 126 });
  expect(placeAttachedProperty({ x: 10, y: 4, h: 40 }, { h: 36 }, workspace, "above")).toEqual({ x: 10, y: 50 });
});

test("puts the highlight bar outside the toolbar and keeps it on screen", () => {
  const workspace = { x: 0, y: 0, w: 1000, h: 800 };
  expect(placeOuterPropertyBar({ x: 10, y: 80, h: 40 }, { h: 36 }, "below", workspace)).toEqual({ x: 10, y: 126 });
  expect(placeOuterPropertyBar({ x: 10, y: 80, h: 40 }, { h: 36 }, "above", workspace)).toEqual({ x: 10, y: 38 });
  expect(placeOuterPropertyBar({ x: 10, y: 760, h: 40 }, { h: 36 }, "below", workspace)).toEqual({ x: 10, y: 718 });
});

test("moves the magnifier and size label when they leave the workspace", () => {
  expect(placeMagnifier({ x: 10, y: 10 }, { x: 0, y: 0, w: 400, h: 300 })).toEqual({ x: 26, y: 26 });
  expect(placeMagnifier({ x: 300, y: 250 }, { x: 0, y: 0, w: 400, h: 300 })).toEqual({ x: 156, y: 106 });
  const selection = { x: 100, y: 100, w: 80, h: 40 };
  expect(placeSizeLabel(selection, { w: 72, h: 22 }, { x: 0, y: 0, w: 400, h: 300 })).toEqual({ x: 188, y: 148 });
  expect(placeSizeLabel(selection, { w: 72, h: 22 }, { x: 0, y: 0, w: 200, h: 150 }).y).toBe(70);
});

test("keeps mosaic blocks aligned to the image origin", () => {
  expect(blockOrigin(1, 8)).toBe(0);
  expect(blockOrigin(8, 8)).toBe(8);
  expect(blocksForRect({ x: 1, y: 1, w: 1, h: 1 }, 8, 16, 16)).toEqual([{ x: 0, y: 0 }]);
});

test("averages a mosaic block once when the same cell is listed twice", () => {
  const pixels = new Uint8ClampedArray([
    255, 0, 0, 255,
    0, 0, 255, 255,
    0, 255, 0, 255,
    255, 255, 255, 255,
  ]);
  const once = applyMosaic(pixels, pixels, 2, 2, [{ x: 0, y: 0 }], 2);
  const twice = applyMosaic(pixels, pixels, 2, 2, [{ x: 0, y: 0 }, { x: 0, y: 0 }], 2);
  expect(dedupeBlocks([{ x: 0, y: 0 }, { x: 0, y: 0 }])).toEqual([{ x: 0, y: 0 }]);
  expect(Array.from(once)).toEqual(Array.from(twice));
  expect(once[0]).toBe(128);
  expect(once[1]).toBe(128);
  expect(once[2]).toBe(128);
});

test("stops an arrow shaft at the base of a 26 degree head", () => {
  const head = arrowHead(0, 0, 40, 0, 4);
  expect(head.tip).toEqual({ x: 40, y: 0 });
  expect(head.shaft.x).toBeCloseTo(24);
  expect(head.shaft.y).toBeCloseTo(0);
  expect(head.left.y).toBeCloseTo(16 * Math.tan((13 * Math.PI) / 180));
  expect(head.right.y).toBeCloseTo(-16 * Math.tan((13 * Math.PI) / 180));
});

test("places a tag bubble on the requested side of its anchor", () => {
  expect(tagBubble(100, 50, 20, 10, "right")).toEqual({ x: 112, y: 37, w: 36, h: 26 });
  expect(tagBubble(100, 50, 20, 10, "left").x).toBe(52);
});

test("uses the remaining selection width for text and falls back to 80", () => {
  expect(textBoxWidth(0, { x: 0, y: 0, w: 300, h: 100 })).toBe(240);
  expect(textBoxWidth(250, { x: 0, y: 0, w: 300, h: 100 })).toBe(80);
});

test("fits a dragged text box inside the image and keeps a minimum size", () => {
  expect(fittedBox({ x: 10, y: 10 }, { x: 110, y: 70 }, 200, 200, 48, 20)).toEqual({ x: 10, y: 10, w: 100, h: 60 });
  expect(fittedBox({ x: 180, y: 10 }, { x: 190, y: 20 }, 200, 200, 48, 30)).toEqual({ x: 152, y: 10, w: 48, h: 30 });
});

test("resizes a text box from the dragged edge without collapsing it", () => {
  expect(resizeTextBox({ x: 10, y: 10, w: 80, h: 40 }, "e", { x: 200, y: 10 }, 400, 300, 48, 20)).toEqual({ x: 10, y: 10, w: 190, h: 40 });
  expect(resizeTextBox({ x: 10, y: 10, w: 80, h: 40 }, "e", { x: 20, y: 10 }, 400, 300, 48, 20)?.w).toBe(48);
});

test("canvas text uses a real font at the selected size", () => {
  expect(canvasFont(36)).toBe('36px -apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif');
  expect(canvasFont(Number.NaN).startsWith("36px ")).toBe(true);
});

test("keeps typed text when a snapshot omits it", () => {
  const text = { kind: "text" as const, x: 4, y: 6, w: 80, text: "你好", size: "md" as const, fontSize: 36, color: "red" as const };
  const local = { monitorIndex: 0, selection: { x: 0, y: 0, w: 100, h: 40 }, marks: [text], watermark: null };
  const incoming = { ...local, marks: [] as typeof local.marks };
  expect(keepsLocalInk(local, incoming)).toBe(true);
  expect(keepsLocalInk(incoming, local)).toBe(false);
  expect(mergeInkMarks(incoming, local.marks).marks).toEqual([text]);
  expect(mergeInkMarks(local, local.marks)).toBe(local);
});

test("keeps the chosen text size when a round trip drops it", () => {
  const text = { kind: "text" as const, x: 1, y: 2, w: 80, text: "你好", size: "md" as const, fontSize: 64, color: "red" as const };
  const previous = { monitorIndex: 0, selection: null, marks: [text], watermark: null };
  const received = { ...previous, marks: [{ ...text, fontSize: undefined as unknown as number }] };
  expect(keepTextSize(previous, received).marks[0]).toMatchObject({ fontSize: 64, text: "你好" });
  expect(keepTextSize(previous, previous)).toBe(previous);
});

test("keeps a trimmed watermark inside the opacity range", () => {
  expect(watermarkPayload({ text: "  hello  ", opacity: 0.44, color: "white" })).toEqual({ text: "hello", opacity: 0.4, color: "white" });
  expect(watermarkPayload({ text: "   ", opacity: 0.4, color: "red" })).toBeNull();
  expect(watermarkPayload({ text: "x".repeat(50), opacity: 2, color: "black" })).toEqual({ text: "x".repeat(40), opacity: 1, color: "black" });
  expect(watermarkPayload({ text: "x", opacity: 0.04, color: "black" })?.opacity).toBe(0.1);
});

test("rejects a frame whose stride does not match the image", () => {
  const bytes = new Uint8Array(16 + 8);
  bytes.set([80, 88, 70, 82]);
  const view = new DataView(bytes.buffer);
  view.setUint32(4, 1, true);
  view.setUint32(8, 2, true);
  view.setUint32(12, 4, true);
  bytes.fill(9, 16);
  expect(parseFrame(bytes).pixels).toHaveLength(8);
  view.setUint32(12, 8, true);
  expect(() => parseFrame(bytes)).toThrow("capture_failed");
});

test("moves a pinned image by the pointer delta", () => {
  expect(draggedOrigin({ x: 10, y: 20 }, { x: 100, y: 200 }, { x: 130, y: 180 })).toEqual({ x: 40, y: 0 });
});

test("reads overlay and pin slots from window labels", () => {
  expect(overlayIndex("screenshot-overlay-2")).toBe(1);
  expect(overlayIndex("screenshot-overlay-9")).toBeNull();
  expect(shotMonitorIndex("screenshot-overlay-m0-1", "?view=shot&monitor=0")).toBe(0);
  expect(shotMonitorIndex("screenshot-overlay-2", "")).toBe(1);
  expect(pinSlot("screenshot-pin-8")).toBe(8);
  expect(pinSlot("screenshot-overlay-1")).toBeNull();
});

test("rounds the exported crop to the frozen image", () => {
  expect(exportBounds({ x: 1.2, y: 3.6, w: 10.4, h: 8.2 }, 20, 20)).toEqual({ x: 1, y: 4, w: 10, h: 8 });
});
