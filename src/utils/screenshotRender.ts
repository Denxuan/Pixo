import { ScreenshotRequestError } from "@/api/screenshot";
import type { EditorDocument, Mark, PaletteColor, Point, Rect, StrokeSize, Watermark } from "@/types/screenshot";
import {
  PALETTE,
  applyMosaic,
  arrowHead,
  blocksForRect,
  blocksForStroke,
  canvasFont,
  exportBounds,
  mosaicBlock,
  strokeWidth,
  tagBubble,
  textSize,
} from "@/utils/screenshotGeometry";

const HIGHLIGHT_ALPHA = 0.45;
const OCR_LONGEST_EDGE = 1600;

const frameCache = new WeakMap<ImageData, { key: string; canvas: HTMLCanvasElement }>();

function mosaicKey(marks: Mark[]) {
  let key = "";
  for (const mark of marks) {
    if (mark.kind === "mosaicRect") key += `r${mark.size}:${mark.x}:${mark.y}:${mark.w}:${mark.h};`;
    else if (mark.kind === "mosaicPen") {
      const last = mark.points[mark.points.length - 1];
      key += `p${mark.size}:${mark.points.length}:${last?.x}:${last?.y};`;
    }
  }
  return key;
}

function cachedBase(source: ImageData, marks: Mark[]) {
  const key = mosaicKey(marks);
  const cached = frameCache.get(source);
  if (cached?.key === key) return cached.canvas;
  const canvas = cached?.canvas ?? document.createElement("canvas");
  if (canvas.width !== source.width) canvas.width = source.width;
  if (canvas.height !== source.height) canvas.height = source.height;
  const base = canvas.getContext("2d");
  if (!base) return null;
  base.putImageData(mosaicFrame(source, marks), 0, 0);
  frameCache.set(source, { key, canvas });
  return canvas;
}

export function paintOverlay(
  ctx: CanvasRenderingContext2D,
  source: ImageData,
  selection: Rect | null,
  marks: Mark[],
  watermark: Watermark | null,
  hover: Rect | null,
  outline = true,
) {
  const base = cachedBase(source, marks);
  if (base) {
    ctx.save();
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(base, 0, 0);
    ctx.restore();
  } else {
    ctx.putImageData(mosaicFrame(source, marks), 0, 0);
  }
  if (selection && outline) dimOutside(ctx, selection, source.width, source.height);
  for (const mark of marks) {
    if (mark.kind !== "mosaicPen" && mark.kind !== "mosaicRect") drawMark(ctx, mark);
  }
  if (selection && watermark && watermark.text.trim()) drawWatermark(ctx, watermark, selection);
  if (!outline) return;
  if (selection) strokeBox(ctx, selection);
  else if (hover) strokeBox(ctx, hover);
}

export async function selectionPng(source: ImageData, editor: EditorDocument, plain = false) {
  const selection = editor.selection;
  if (!selection) throw new ScreenshotRequestError("invalid_input");
  const bounds = exportBounds(selection, source.width, source.height);
  const longest = Math.max(bounds.w, bounds.h);
  const scale = plain && longest > OCR_LONGEST_EDGE ? OCR_LONGEST_EDGE / longest : 1;
  const width = scale === 1 ? bounds.w : Math.max(1, Math.round(bounds.w * scale));
  const height = scale === 1 ? bounds.h : Math.max(1, Math.round(bounds.h * scale));
  const mosaiced = plain ? source : mosaicFrame(source, editor.marks);
  const canvas = globalThis.document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new ScreenshotRequestError("capture_failed");
  const cropped = crop(mosaiced, bounds);
  const layer = globalThis.document.createElement("canvas");
  layer.width = bounds.w;
  layer.height = bounds.h;
  const layerCtx = layer.getContext("2d");
  if (!layerCtx) throw new ScreenshotRequestError("capture_failed");
  layerCtx.putImageData(cropped, 0, 0);
  if (scale === 1) ctx.drawImage(layer, 0, 0);
  else {
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(layer, 0, 0, width, height);
  }
  if (!plain) {
    ctx.save();
    ctx.translate(-bounds.x, -bounds.y);
    ctx.beginPath();
    ctx.rect(bounds.x, bounds.y, bounds.w, bounds.h);
    ctx.clip();
    for (const mark of editor.marks) {
      if (mark.kind !== "mosaicPen" && mark.kind !== "mosaicRect") drawMark(ctx, mark);
    }
    if (editor.watermark?.text.trim()) drawWatermark(ctx, editor.watermark, { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h });
    ctx.restore();
  }
  const hasInk = !plain && (editor.marks.some((mark) => mark.kind !== "mosaicPen" && mark.kind !== "mosaicRect") || Boolean(editor.watermark?.text.trim()));
  if (hasInk) {
    const burned = ctx.getImageData(0, 0, bounds.w, bounds.h);
    ctx.putImageData(burned, 0, 0);
  }
  const png = await encodeCanvasPng(canvas);
  return { png, ...bounds };
}

// toDataURL compresses on the webview thread. A retina selection stalls the overlay for the whole encode.
async function encodeCanvasPng(canvas: HTMLCanvasElement) {
  if (typeof Worker === "undefined" || typeof createImageBitmap !== "function") return canvasToPngBase64(canvas);
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(canvas);
  } catch {
    return canvasToPngBase64(canvas);
  }
  try {
    return await encodeBitmap(bitmap);
  } catch {
    try {
      bitmap.close();
    } catch {
      // The worker already owns the bitmap.
    }
    return canvasToPngBase64(canvas);
  }
}

function encodeBitmap(bitmap: ImageBitmap) {
  return new Promise<string>((resolve, reject) => {
    const worker = new Worker(new URL("./pngEncode.worker.ts", import.meta.url), { type: "module" });
    let settled = false;
    const finish = (png?: string, error?: ScreenshotRequestError) => {
      if (settled) return;
      settled = true;
      worker.terminate();
      if (png) resolve(png);
      else reject(error ?? new ScreenshotRequestError("capture_failed"));
    };
    worker.onmessage = (event: MessageEvent<{ png?: string }>) => {
      if (event.data?.png) finish(event.data.png);
      else finish(undefined, new ScreenshotRequestError("capture_failed"));
    };
    worker.onerror = () => finish(undefined, new ScreenshotRequestError("capture_failed"));
    try {
      worker.postMessage({ bitmap }, [bitmap]);
    } catch {
      finish(undefined, new ScreenshotRequestError("capture_failed"));
    }
  });
}

function canvasToPngBase64(canvas: HTMLCanvasElement) {
  return new Promise<string>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        reject(new ScreenshotRequestError("capture_failed"));
        return;
      }
      const reader = new FileReader();
      reader.onload = () => {
        const url = String(reader.result ?? "");
        const comma = url.indexOf(",");
        if (comma < 0) reject(new ScreenshotRequestError("capture_failed"));
        else resolve(url.slice(comma + 1));
      };
      reader.onerror = () => reject(new ScreenshotRequestError("capture_failed"));
      reader.readAsDataURL(blob);
    }, "image/png");
  });
}

export function magnifierPatch(source: ImageData, x: number, y: number) {
  const size = 32;
  const left = Math.round(x) - 16;
  const top = Math.round(y) - 16;
  const pixels = new Uint8ClampedArray(size * size * 4);
  for (let row = 0; row < size; row += 1) {
    for (let col = 0; col < size; col += 1) {
      const sx = left + col;
      const sy = top + row;
      const dest = (row * size + col) * 4;
      if (sx < 0 || sy < 0 || sx >= source.width || sy >= source.height) {
        pixels[dest + 3] = 255;
        continue;
      }
      pixels.set(source.data.subarray((sy * source.width + sx) * 4, (sy * source.width + sx) * 4 + 4), dest);
    }
  }
  return new ImageData(pixels, size, size);
}

function mosaicFrame(source: ImageData, marks: Mark[]) {
  const grouped = new Map<number, Array<{ x: number; y: number }>>();
  for (const mark of marks) {
    if (mark.kind === "mosaicRect") {
      const block = mosaicBlock(mark.size);
      const list = grouped.get(block) ?? [];
      list.push(...blocksForRect(mark, block, source.width, source.height));
      grouped.set(block, list);
    } else if (mark.kind === "mosaicPen") {
      const block = mosaicBlock(mark.size);
      const list = grouped.get(block) ?? [];
      list.push(...blocksForStroke(mark.points, block, source.width, source.height));
      grouped.set(block, list);
    }
  }
  if (grouped.size === 0) return source;
  let pixels = source.data;
  for (const [block, cells] of grouped) {
    pixels = applyMosaic(pixels, source.data, source.width, source.height, cells, block);
  }
  return new ImageData(pixels, source.width, source.height);
}

function crop(image: ImageData, bounds: { x: number; y: number; w: number; h: number }) {
  const out = new Uint8ClampedArray(bounds.w * bounds.h * 4);
  for (let y = 0; y < bounds.h; y += 1) {
    const src = ((bounds.y + y) * image.width + bounds.x) * 4;
    out.set(image.data.subarray(src, src + bounds.w * 4), y * bounds.w * 4);
  }
  return new ImageData(out, bounds.w, bounds.h);
}

function dimOutside(ctx: CanvasRenderingContext2D, selection: Rect, width: number, height: number) {
  ctx.fillStyle = "rgba(0, 0, 0, 0.45)";
  ctx.fillRect(0, 0, width, selection.y);
  ctx.fillRect(0, selection.y, selection.x, selection.h);
  ctx.fillRect(selection.x + selection.w, selection.y, width - selection.x - selection.w, selection.h);
  ctx.fillRect(0, selection.y + selection.h, width, height - selection.y - selection.h);
}

function strokeBox(ctx: CanvasRenderingContext2D, rect: Rect) {
  ctx.save();
  ctx.strokeStyle = PALETTE.blue;
  ctx.lineWidth = 2;
  ctx.strokeRect(rect.x + 1, rect.y + 1, Math.max(0, rect.w - 2), Math.max(0, rect.h - 2));
  ctx.restore();
}

function drawMark(ctx: CanvasRenderingContext2D, mark: Mark) {
  if (mark.kind === "rect" || mark.kind === "ellipse") {
    strokeShape(ctx, mark);
    return;
  }
  if (mark.kind === "arrow") {
    drawArrow(ctx, mark.x1, mark.y1, mark.x2, mark.y2, strokeWidth(mark.size), PALETTE[mark.color]);
    return;
  }
  if (mark.kind === "pen" || mark.kind === "highlight") {
    strokePoints(ctx, mark.points, strokeWidth(mark.size), PALETTE[mark.color], mark.kind === "highlight" ? HIGHLIGHT_ALPHA : 1);
    return;
  }
  if (mark.kind === "text") {
    drawText(ctx, mark.x, mark.y, mark.w, mark.text, mark.fontSize || textSize(mark.size), PALETTE[mark.color]);
    return;
  }
  if (mark.kind === "tag") drawTag(ctx, mark);
}

function strokeShape(ctx: CanvasRenderingContext2D, mark: { x: number; y: number; w: number; h: number; size: StrokeSize; color: PaletteColor; kind: "rect" | "ellipse" }) {
  ctx.save();
  ctx.strokeStyle = PALETTE[mark.color];
  ctx.lineWidth = strokeWidth(mark.size);
  ctx.beginPath();
  if (mark.kind === "ellipse") ctx.ellipse(mark.x + mark.w / 2, mark.y + mark.h / 2, Math.abs(mark.w) / 2, Math.abs(mark.h) / 2, 0, 0, Math.PI * 2);
  else ctx.rect(mark.x, mark.y, mark.w, mark.h);
  ctx.stroke();
  ctx.restore();
}

function strokePoints(ctx: CanvasRenderingContext2D, points: Point[], width: number, color: string, alpha: number) {
  if (points.length < 2) return;
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.strokeStyle = color;
  ctx.lineWidth = width;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (const point of points.slice(1)) ctx.lineTo(point.x, point.y);
  ctx.stroke();
  ctx.restore();
}

function drawArrow(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number, width: number, color: string) {
  const head = arrowHead(x1, y1, x2, y2, width);
  ctx.save();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = width;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(head.shaft.x, head.shaft.y);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(head.tip.x, head.tip.y);
  ctx.lineTo(head.left.x, head.left.y);
  ctx.lineTo(head.right.x, head.right.y);
  ctx.closePath();
  ctx.fill();
  ctx.restore();
}

function drawText(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, text: string, size: number, color: string) {
  ctx.save();
  ctx.fillStyle = color;
  ctx.font = canvasFont(size);
  ctx.textBaseline = "top";
  const lines = wrapText(ctx, text, Math.max(1, width));
  lines.forEach((line, index) => ctx.fillText(line, x, y + index * size * 1.4));
  ctx.restore();
}

function drawTag(ctx: CanvasRenderingContext2D, mark: Extract<Mark, { kind: "tag" }>) {
  const size = textSize(mark.size);
  ctx.save();
  ctx.font = canvasFont(size);
  const lines = wrapText(ctx, mark.text, 240 - 16);
  const textWidth = Math.min(240 - 16, Math.max(...lines.map((line) => ctx.measureText(line).width), 0));
  const textHeight = Math.max(lines.length, 1) * size * 1.4;
  const bubble = tagBubble(mark.anchorX, mark.anchorY, textWidth, textHeight, mark.side);
  ctx.fillStyle = PALETTE[mark.color];
  ctx.beginPath();
  ctx.arc(mark.anchorX, mark.anchorY, 6, 0, Math.PI * 2);
  ctx.fill();
  roundRect(ctx, bubble.x, bubble.y, bubble.w, bubble.h, 6);
  ctx.fill();
  ctx.fillStyle = textOn(mark.color);
  ctx.textBaseline = "top";
  lines.forEach((line, index) => ctx.fillText(line, bubble.x + 8, bubble.y + 8 + index * size * 1.4));
  ctx.restore();
}

function drawWatermark(ctx: CanvasRenderingContext2D, watermark: Watermark, selection: Rect) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(selection.x, selection.y, selection.w, selection.h);
  ctx.clip();
  ctx.translate(selection.x + selection.w / 2, selection.y + selection.h / 2);
  ctx.rotate((-24 * Math.PI) / 180);
  ctx.globalAlpha = watermark.opacity;
  ctx.fillStyle = PALETTE[watermark.color];
  ctx.font = canvasFont(22);
  ctx.textBaseline = "middle";
  const text = watermark.text.slice(0, 40);
  const width = ctx.measureText(text).width + 48;
  const height = 22 + 48;
  const span = Math.hypot(selection.w, selection.h);
  for (let y = -span; y <= span; y += height) {
    for (let x = -span; x <= span; x += width) ctx.fillText(text, x, y);
  }
  ctx.restore();
}

function wrapText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number) {
  const rows: string[] = [];
  for (const line of text.split("\n")) {
    if (line.length === 0) {
      rows.push("");
      continue;
    }
    let current = "";
    for (const char of line) {
      const next = current + char;
      if (current && ctx.measureText(next).width > maxWidth) {
        rows.push(current);
        current = char;
      } else current = next;
    }
    rows.push(current);
  }
  return rows.length > 0 ? rows : [""];
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, radius: number) {
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + w, y, x + w, y + h, radius);
  ctx.arcTo(x + w, y + h, x, y + h, radius);
  ctx.arcTo(x, y + h, x, y, radius);
  ctx.arcTo(x, y, x + w, y, radius);
  ctx.closePath();
}

function textOn(color: PaletteColor) {
  return color === "black" || color === "red" || color === "blue" ? "#FFFFFF" : "#1C1C1C";
}
