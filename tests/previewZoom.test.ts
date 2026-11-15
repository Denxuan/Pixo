import { expect, test } from "vitest";

import { clampPan, clampZoom, pinnedPictureStyle, zoomAt } from "@/utils/previewZoom";

test("zoom stays on the point under the cursor", () => {
  const cursor = { x: 10, y: -4 };
  const next = zoomAt(1, { x: 0, y: 0 }, 2, cursor);
  expect(next.scale).toBe(2);
  const before = { x: (cursor.x - 0) / 1, y: (cursor.y - 0) / 1 };
  expect(before.x * next.scale + next.pan.x).toBeCloseTo(cursor.x);
  expect(before.y * next.scale + next.pan.y).toBeCloseTo(cursor.y);
});

test("returning to the fitted size clears the pan", () => {
  const next = zoomAt(2, { x: -10, y: 6 }, 1, { x: 4, y: 4 });
  expect(next).toEqual({ scale: 1, pan: { x: 0, y: 0 } });
});

test("zoom does not pass the limits", () => {
  expect(clampZoom(0.2)).toBe(1);
  expect(clampZoom(20)).toBe(6);
  expect(zoomAt(4, { x: 0, y: 0 }, 40, { x: 0, y: 0 }).scale).toBe(6);
});

test("pan stays within the scaled picture", () => {
  expect(clampPan({ x: 80, y: -40 }, 2, 100, 50)).toEqual({ x: 50, y: -25 });
});

test("a pinned picture scales around the selection center", () => {
  const crop = { x: 40, y: 20, w: 200, h: 100 };
  expect(pinnedPictureStyle(crop, 1, { x: 12, y: 8 })).toEqual({});
  expect(pinnedPictureStyle(crop, 2, { x: 12, y: -4 })).toEqual({
    transform: "translate(12px, -4px) scale(2)",
    transformOrigin: "140px 70px",
  });
});
