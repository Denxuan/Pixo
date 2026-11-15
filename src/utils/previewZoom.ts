export const MIN_ZOOM = 1;
export const MAX_ZOOM = 6;

export type Pan = { x: number; y: number };

export function clampZoom(scale: number) {
  if (!Number.isFinite(scale)) return MIN_ZOOM;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, scale));
}

/** Keep the point under the cursor fixed while the scale changes. */
export function zoomAt(scale: number, pan: Pan, next: number, cursor: Pan): { scale: number; pan: Pan } {
  const clamped = clampZoom(next);
  if (clamped <= MIN_ZOOM || scale <= 0) return { scale: MIN_ZOOM, pan: { x: 0, y: 0 } };
  const ratio = clamped / scale;
  return {
    scale: clamped,
    pan: {
      x: cursor.x - (cursor.x - pan.x) * ratio,
      y: cursor.y - (cursor.y - pan.y) * ratio,
    },
  };
}

export function clampPan(pan: Pan, scale: number, width: number, height: number): Pan {
  const limitX = Math.max(0, (width * (scale - 1)) / 2);
  const limitY = Math.max(0, (height * (scale - 1)) / 2);
  return {
    x: Math.min(limitX, Math.max(-limitX, pan.x)),
    y: Math.min(limitY, Math.max(-limitY, pan.y)),
  };
}

/** Scale the pinned picture around the selection center. The window itself stays the selection size. */
export function pinnedPictureStyle(
  crop: { x: number; y: number; w: number; h: number },
  scale: number,
  pan: Pan,
): { transform?: string; transformOrigin?: string } {
  const clamped = clampZoom(scale);
  if (clamped <= MIN_ZOOM) return {};
  return {
    transform: `translate(${pan.x}px, ${pan.y}px) scale(${clamped})`,
    transformOrigin: `${crop.x + crop.w / 2}px ${crop.y + crop.h / 2}px`,
  };
}
