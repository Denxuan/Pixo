import { listen } from "@tauri-apps/api/event";
import { LogicalPosition, getCurrentWindow } from "@tauri-apps/api/window";

import {
  ScreenshotRequestError,
  screenshotAdoptPin,
  screenshotCancel,
  screenshotCopy,
  screenshotCopyText,
  screenshotCurrent,
  screenshotEditorCommit,
  screenshotEditorRedo,
  screenshotEditorUndo,
  screenshotFinish,
  screenshotFrame,
  screenshotRecognizeText,
  screenshotOverlayReady,
  screenshotRecordStart,
  screenshotReleasePin,
  screenshotSave,
  screenshotScroll,
  screenshotScrollStop,
  toScreenshotError,
} from "@/api/screenshot";
import type { EditorDocument, Mark, PaletteColor, Point, Rect, SessionView, ShotTool, StrokeSize, TagSide, Watermark } from "@/types/screenshot";
import {
  CANVAS_FONT_FAMILY,
  PALETTE,
  keepTextSize,
  keepsLocalInk,
  mergeInkMarks,
  sameInkMark,
  clampPoint,
  clampSelection,
  contains,
  draggedOrigin,
  fittedBox,
  handlePoints,
  hitHandle,
  imagePoint,
  isClick,
  moveSelection,
  shotMonitorIndex,
  parseFrame,
  pixelHex,
  placeMagnifier,
  placePropertyBar,
  placeSizeLabel,
  placeToolbar,
  resizeSelection,
  resizeTextBox,
  textBoxWidth,
  textSize,
  topWindow,
  watermarkPayload,
  windowLocal,
  type Handle,
} from "@/utils/screenshotGeometry";
import { clampPan, pinnedPictureStyle, zoomAt, type Pan } from "@/utils/previewZoom";
import { magnifierPatch, paintOverlay, selectionPng } from "@/utils/screenshotRender";
import { screenshotErrorText } from "@/utils/screenshotMessage";

type Style = { color: PaletteColor; size: StrokeSize };
type Draft =
  | { id: number; kind: "text"; x: number; y: number; w: number; h: number; text: string }
  | { id: number; kind: "tag"; x: number; y: number; text: string; side: TagSide };
type Gesture =
  | { kind: "select"; start: Point }
  | { kind: "move"; start: Point; origin: Rect }
  | { kind: "resize"; handle: Handle; origin: Rect }
  | { kind: "textbox"; start: Point }
  | { kind: "shape"; start: Point }
  | { kind: "stroke"; start: Point; points: Point[] };

const COLORS: PaletteColor[] = ["red", "yellow", "green", "blue", "black", "white"];
const SIZES: StrokeSize[] = ["sm", "md", "lg"];
const DRAW_TOOLS: ShotTool[] = ["rect", "ellipse", "arrow", "pen", "highlight", "mosaic", "text", "tag", "watermark"];
const EMPTY_WATERMARK: Watermark = { text: "", opacity: 0.4, color: "white" };
const EDITABLE_STYLE = { WebkitUserSelect: "text", userSelect: "text" } as const;
const TEXT_FONTS = [24, 36, 48, 64, 80, 96];
const MIN_TEXT_BOX = 48;
const TEXT_HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const TEXT_HANDLE_CLASS: Record<Handle, string> = {
  nw: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-nwse-resize border border-[#3B82F6] bg-white p-0",
  n: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-ns-resize border border-[#3B82F6] bg-white p-0",
  ne: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-nesw-resize border border-[#3B82F6] bg-white p-0",
  e: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-ew-resize border border-[#3B82F6] bg-white p-0",
  se: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-nwse-resize border border-[#3B82F6] bg-white p-0",
  s: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-ns-resize border border-[#3B82F6] bg-white p-0",
  sw: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-nesw-resize border border-[#3B82F6] bg-white p-0",
  w: "pointer-events-auto absolute z-20 h-2 w-2 -translate-x-1/2 -translate-y-1/2 cursor-ew-resize border border-[#3B82F6] bg-white p-0",
};
const TEXT_HANDLE_STYLE: Record<Handle, { left: string; top: string }> = {
  nw: { left: "0%", top: "0%" },
  n: { left: "50%", top: "0%" },
  ne: { left: "100%", top: "0%" },
  e: { left: "100%", top: "50%" },
  se: { left: "100%", top: "100%" },
  s: { left: "50%", top: "100%" },
  sw: { left: "0%", top: "100%" },
  w: { left: "0%", top: "50%" },
};

function readIndex() {
  try {
    return shotMonitorIndex(getCurrentWindow().label, window.location.search);
  } catch {
    return null;
  }
}

function freshStyles(): Record<ShotTool, Style> {
  const style = { color: "red" as const, size: "md" as const };
  return {
    adjust: style,
    rect: { ...style },
    ellipse: { ...style },
    arrow: { ...style },
    pen: { ...style },
    highlight: { ...style },
    mosaic: { ...style },
    text: { ...style },
    tag: { ...style },
    watermark: { color: "white", size: "md" },
  };
}

function draftToMark(snapshot: Draft, fontSize: number, textStyle: Style, tagStyle: Style, bounds?: { width: number; height: number } | null): Mark {
  if (snapshot.kind === "text") {
    const font = Math.min(200, Math.max(12, Number.isFinite(fontSize) ? fontSize : 36));
    let x = Number.isFinite(snapshot.x) ? snapshot.x : 0;
    let y = Number.isFinite(snapshot.y) ? snapshot.y : 0;
    let w = Number.isFinite(snapshot.w) ? Math.max(2, snapshot.w) : 2;
    if (bounds && bounds.width >= 2 && bounds.height >= 2) {
      x = Math.min(Math.max(0, x), bounds.width - 2);
      y = Math.min(Math.max(0, y), bounds.height - 2);
      w = Math.min(Math.max(2, w), Math.max(2, bounds.width - x));
    }
    return { kind: "text", x, y, w, text: snapshot.text.slice(0, 4000), size: "md", fontSize: font, color: textStyle.color };
  }
  let x = Number.isFinite(snapshot.x) ? snapshot.x : 0;
  let y = Number.isFinite(snapshot.y) ? snapshot.y : 0;
  if (bounds && bounds.width >= 1 && bounds.height >= 1) {
    x = Math.min(Math.max(0, x), Math.max(0, bounds.width - 1));
    y = Math.min(Math.max(0, y), Math.max(0, bounds.height - 1));
  }
  return { kind: "tag", anchorX: x, anchorY: y, text: snapshot.text.slice(0, 4000), size: tagStyle.size, color: tagStyle.color, side: snapshot.side };
}

function boxOf(start: Point, end: Point, selection: Rect, width: number, height: number) {
  const a = clampPoint(start, selection);
  const b = clampPoint(end, selection);
  return clampSelection({ x: a.x, y: a.y, w: b.x - a.x, h: b.y - a.y }, width, height);
}

export default function ScreenshotOverlay() {
  const { t } = useTranslation();
  const [index] = useState(readIndex);
  const [view, setView] = useState<SessionView | null>(null);
  const [image, setImage] = useState<ImageData | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [tool, setTool] = useState<ShotTool>("adjust");
  const [shotMode, setShotMode] = useState<"capture" | "record" | "scroll">("capture");
  const [countdown, setCountdown] = useState<number | null>(null);
  const [styles, setStyles] = useState(freshStyles);
  const [mosaicMode, setMosaicMode] = useState<"brush" | "box">("brush");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [heldMarks, setHeldMarks] = useState<Mark[]>([]);
  const [textFrame, setTextFrame] = useState<Rect | null>(null);
  const [textFont, setTextFont] = useState(36);
  const [watermark, setWatermark] = useState<Watermark>(EMPTY_WATERMARK);
  const [previewSelection, setPreviewSelection] = useState<Rect | null>(null);
  const [previewMark, setPreviewMark] = useState<Mark | null>(null);
  const [clearMarks, setClearMarks] = useState(false);
  const [hover, setHover] = useState<Rect | null>(null);
  const [toolbarSize, setToolbarSize] = useState({ w: 640, h: 40 });
  const [recordBarSize, setRecordBarSize] = useState({ w: 228, h: 40 });
  const [propertySize, setPropertySize] = useState({ w: 280, h: 36 });
  const rootRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const magnifierRef = useRef<HTMLCanvasElement>(null);
  const magnifierBoxRef = useRef<HTMLDivElement>(null);
  const colorRef = useRef<HTMLDivElement>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);
  const recordBarRef = useRef<HTMLDivElement>(null);
  const propertyRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<SessionView | null>(null);
  const imageRef = useRef<ImageData | null>(null);
  const toolRef = useRef(tool);
  const shotModeRef = useRef(shotMode);
  const countdownToken = useRef(0);
  const countingRef = useRef(false);
  const stylesRef = useRef(styles);
  const mosaicRef = useRef(mosaicMode);
  const draftRef = useRef<Draft | null>(null);
  const heldRef = useRef<Mark[]>([]);
  const draftFieldRef = useRef<HTMLTextAreaElement>(null);
  const draftIdRef = useRef(0);
  const armingRef = useRef(false);
  const textResizeRef = useRef<{ handle: Handle; origin: Rect } | null>(null);
  const textFontRef = useRef(textFont);
  const scaleRef = useRef(1);
  const pendingPoint = useRef<Point | null>(null);
  const pointerJob = useRef(0);
  const frameJob = useRef(0);
  const frameTask = useRef<(() => void) | null>(null);
  const watermarkRef = useRef(watermark);
  const gestureRef = useRef<Gesture | null>(null);
  const draggingRef = useRef(false);
  const editQueue = useRef(Promise.resolve());
  const commitErrorRef = useRef<ScreenshotRequestError | null>(null);
  if (!view) viewRef.current = null;
  else if (!viewRef.current || view.id !== viewRef.current.id || view.version > viewRef.current.version) viewRef.current = view;
  else if (view.version === viewRef.current.version && view.document.marks.length === viewRef.current.document.marks.length && !keepsLocalInk(viewRef.current.document, view.document)) viewRef.current = view;
  imageRef.current = image;
  toolRef.current = tool;
  shotModeRef.current = shotMode;
  stylesRef.current = styles;
  mosaicRef.current = mosaicMode;
  if (draft && (!draftRef.current || draft.id >= draftRef.current.id)) draftRef.current = draft;
  watermarkRef.current = watermark;
  textFontRef.current = textFont;

  const monitor = index == null ? null : view?.monitors[index] ?? null;
  const scale = monitor?.scaleFactor || 1;
  scaleRef.current = scale;
  const shown = previewSelection ?? (view?.document.monitorIndex === index ? view.document.selection : null);

  const loadGen = useRef(0);
  const copyingRef = useRef(false);
  const readingRef = useRef(false);
  const scrollingRef = useRef(false);
  const copyingOcrRef = useRef(false);
  const ocrToken = useRef(0);
  const [reading, setReading] = useState(false);
  const [scrolling, setScrolling] = useState(false);
  const [ocrText, setOcrText] = useState<string | null>(null);
  const pinningRef = useRef(false);
  const pinnedRef = useRef(false);
  const pngTask = useRef<Promise<string> | null>(null);
  const [pinned, setPinned] = useState(false);
  const [pinCrop, setPinCrop] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  const [pinScale, setPinScale] = useState(1);
  const [pinPan, setPinPan] = useState<Pan>({ x: 0, y: 0 });
  const [pinPanning, setPinPanning] = useState(false);
  const pinCropRef = useRef(pinCrop);
  const pinScaleRef = useRef(1);
  const pinPanRef = useRef<Pan>({ x: 0, y: 0 });
  pinCropRef.current = pinCrop;
  const windowsRef = useRef<{ id: number; windows: SessionView["windows"] } | null>(null);

  function applyPinZoom(scale: number, pan: Pan) {
    pinScaleRef.current = scale;
    pinPanRef.current = pan;
    setPinScale(scale);
    setPinPan(pan);
  }

  function zoomPinBy(factor: number) {
    const crop = pinCropRef.current;
    if (!crop) return;
    const next = zoomAt(pinScaleRef.current, pinPanRef.current, pinScaleRef.current * factor, { x: 0, y: 0 });
    applyPinZoom(next.scale, next.scale <= 1 ? { x: 0, y: 0 } : clampPan(next.pan, next.scale, crop.w, crop.h));
  }

  useEffect(() => {
    const node = rootRef.current;
    if (!pinned || !node) return;
    const onWheel = (event: WheelEvent) => {
      const crop = pinCropRef.current;
      if (!crop) return;
      event.preventDefault();
      const cursor = { x: event.clientX - crop.w / 2, y: event.clientY - crop.h / 2 };
      const factor = Math.exp(-event.deltaY * 0.0015);
      const next = zoomAt(pinScaleRef.current, pinPanRef.current, pinScaleRef.current * factor, cursor);
      applyPinZoom(next.scale, next.scale <= 1 ? { x: 0, y: 0 } : clampPan(next.pan, next.scale, crop.w, crop.h));
    };
    node.addEventListener("wheel", onWheel, { passive: false });
    return () => node.removeEventListener("wheel", onWheel);
  }, [pinned]);

  useEffect(() => {
    if (index == null) return;
    let alive = true;
    let stopSession: (() => void) | undefined;
    const load = async () => {
      const gen = ++loadGen.current;
      try {
        const current = await screenshotCurrent();
        if (!alive || gen !== loadGen.current) return;
        const bytes = await screenshotFrame(index);
        if (!alive || gen !== loadGen.current) return;
        const parsed = parseLoadedFrame(bytes);
        gestureRef.current = null;
        draftRef.current = null;
        heldRef.current = [];
        setHeldMarks([]);
        setDraft(null);
        setTextFrame(null);
        setPreviewSelection(null);
        setPreviewMark(null);
        setHover(null);
        hideMagnifier();
        setClearMarks(false);
        setTool("adjust");
        const mode = current.mode === "record" ? "record" : "capture";
        shotModeRef.current = mode;
        setShotMode(mode);
        if (mode === "record") hideMagnifier();
        countdownToken.current += 1;
        countingRef.current = false;
        setCountdown(null);
        setNotice("");
        setError("");
        setImage(parsed);
        const cached = windowsRef.current;
        setView(cached && cached.id === current.id && current.windows.length === 0 ? { ...current, windows: cached.windows } : current);
        setWatermark(current.document.watermark ?? EMPTY_WATERMARK);
      } catch (caught) {
        if (!alive || gen !== loadGen.current) return;
        const parsed = toScreenshotError(caught);
        if (parsed.code === "cancelled") return;
        setError(screenshotErrorText(t, parsed));
      }
    };
    void load();
    void listen<{ phase: string; code: string | null }>("screenshot://session", (event) => {
      if (pinnedRef.current) return;
      if (event.payload.phase === "live") {
        void load();
        return;
      }
      if (event.payload.phase !== "cancelled" && event.payload.phase !== "finished") return;
      countdownToken.current += 1;
      countingRef.current = false;
      setCountdown(null);
      shotModeRef.current = "capture";
      setShotMode("capture");
      loadGen.current += 1;
      setImage(null);
      setView(null);
      setDraft(null);
      setTextFrame(null);
      draftRef.current = null;
      heldRef.current = [];
      setHeldMarks([]);
      if (event.payload.code === "display_changed") setError(t("zdx.text.screenshot.displayChanged"));
      else setError("");
    }).then((stop) => {
      if (alive) stopSession = stop;
      else stop();
    });
    return () => {
      alive = false;
      loadGen.current += 1;
      stopSession?.();
    };
  }, [index, t]);

  useEffect(() => {
    let stopEditor: (() => void) | undefined;
    let stopWindows: (() => void) | undefined;
    let alive = true;
    void listen<{ version: number }>("screenshot://editor", () => {
      if (draggingRef.current) return;
      void screenshotCurrent().then((next) => {
        if (!alive || draggingRef.current) return;
        if (viewRef.current && viewRef.current.id === next.id && viewRef.current.version >= next.version) return;
        showDocument(next);
        const editingWatermark = document.activeElement instanceof HTMLElement && document.activeElement.dataset.shotField === "watermark";
        if (!editingWatermark) setWatermark(next.document.watermark ?? EMPTY_WATERMARK);
      }).catch(() => undefined);
    }).then((stop) => {
      if (alive) stopEditor = stop;
      else stop();
    });
    void listen<{ id: number }>("screenshot://windows", (event) => {
      if (draggingRef.current) return;
      void screenshotCurrent().then((next) => {
        if (!alive || draggingRef.current || next.id !== event.payload.id) return;
        windowsRef.current = { id: next.id, windows: next.windows };
        setView((current) => current && current.id === next.id ? { ...current, windows: next.windows } : current);
      }).catch(() => undefined);
    }).then((stop) => {
      if (alive) stopWindows = stop;
      else stop();
    });
    return () => {
      alive = false;
      stopEditor?.();
      stopWindows?.();
    };
  }, []);

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !image || !view || index == null) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const owns = view.document.monitorIndex === index;
    const marks = clearMarks || !owns ? [] : view.document.marks;
    const retained = clearMarks ? [] : heldMarks.filter((mark) => !marks.some((item) => sameInkMark(item, mark)));
    const pendingMark = !pinned && !clearMarks && draft?.kind === "text" && draft.text ? draftToMark(draft, textFont, styles.text, styles.tag, image) : null;
    const pending = pendingMark && ![...marks, ...retained].some((item) => sameInkMark(item, pendingMark)) ? pendingMark : null;
    const painted = pending ? [...marks, ...retained, pending] : [...marks, ...retained];
    paintOverlay(ctx, image, shown, previewMark && !pinned ? [...painted, previewMark] : painted, owns && !clearMarks ? watermarkPayload(watermark) : null, pinned || shown ? null : hover, !pinned);
  }, [clearMarks, draft, heldMarks, hover, image, index, pinned, previewMark, shown, styles, textFont, view, watermark]);

  const sessionId = view?.id;
  useEffect(() => {
    if (!image || sessionId == null) return;
    let done = false;
    const reveal = () => {
      if (done) return;
      done = true;
      void screenshotOverlayReady().catch(() => undefined);
    };
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(reveal);
    });
    const timer = window.setTimeout(reveal, 200);
    return () => {
      done = true;
      window.clearTimeout(timer);
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, [image, sessionId]);

  useEffect(() => {
    if (!error) return;
    void screenshotOverlayReady().catch(() => undefined);
  }, [error]);

  useLayoutEffect(() => {
    if (toolbarRef.current) setToolbarSize({ w: toolbarRef.current.offsetWidth, h: toolbarRef.current.offsetHeight });
    if (recordBarRef.current) setRecordBarSize({ w: recordBarRef.current.offsetWidth, h: recordBarRef.current.offsetHeight });
    if (propertyRef.current) setPropertySize({ w: propertyRef.current.offsetWidth, h: propertyRef.current.offsetHeight });
  }, [tool, shown, draft, shotMode, previewSelection, countdown]);

  useEffect(() => {
    rootRef.current?.focus();
  }, [image]);

  const draftKey = draft ? `${draft.kind}:${draft.id}` : "";
  useLayoutEffect(() => {
    if (!draftKey) {
      armingRef.current = false;
      window.scrollTo(0, 0);
      return;
    }
    draftFieldRef.current?.focus({ preventScroll: true });
    const timer = window.setTimeout(() => {
      armingRef.current = false;
    }, 80);
    return () => window.clearTimeout(timer);
  }, [draftKey]);

  useLayoutEffect(() => {
    const field = draftFieldRef.current;
    const current = draftRef.current;
    if (!field || current?.kind !== "text" || textResizeRef.current) return;
    const image = imageRef.current;
    const overflow = field.scrollHeight - field.clientHeight;
    if (!image || overflow <= 1) return;
    const room = Math.max(current.h, image.height - current.y);
    const nextH = Math.min(room, current.h + textFontRef.current * 1.4);
    if (nextH <= current.h + 1) return;
    const next = { ...current, h: nextH };
    draftRef.current = next;
    setDraft(next);
  }, [draft?.id, draft?.text, draft && draft.kind === "text" ? draft.w : 0, scale, textFont]);

  function holdMark(mark: Mark) {
    if (heldRef.current.some((item) => sameInkMark(item, mark))) return;
    const next = [...heldRef.current, mark];
    heldRef.current = next;
    setHeldMarks(next);
  }

  function showDocument(next: SessionView, sent?: EditorDocument) {
    const local = viewRef.current;
    if (local && local.id === next.id && local.version > next.version) return local;
    const previous = sent ?? local?.document ?? next.document;
    const sized = keepTextSize(previous, next.document);
    const document = mergeInkMarks(sent ? mergeInkMarks(sized, sent.marks) : sized, heldRef.current);
    const merged = { ...next, document };
    viewRef.current = merged;
    setView(merged);
    const kept = heldRef.current.filter((mark) => !sized.marks.some((item) => sameInkMark(item, mark)));
    if (kept.length !== heldRef.current.length) {
      heldRef.current = kept;
      setHeldMarks(kept);
    }
    return merged;
  }

  function edit(change: (document: EditorDocument) => EditorDocument | null) {
    const run = editQueue.current.then(async () => {
      const base = viewRef.current;
      if (!base) return null;
      const changed = change(mergeInkMarks(base.document, heldRef.current));
      if (!changed) return base;
      const next = mergeInkMarks(changed, heldRef.current);
      const staged = { ...base, document: next };
      viewRef.current = staged;
      setView(staged);
      const saved = await commit(next, base.version);
      if (saved) return saved;
      const fresh = await screenshotCurrent().catch(() => null);
      if (!fresh || fresh.version === base.version) {
        if (commitErrorRef.current) setNotice(screenshotErrorText(t, commitErrorRef.current));
        return viewRef.current;
      }
      const retry = change(mergeInkMarks(keepTextSize(next, fresh.document), heldRef.current));
      if (!retry) return viewRef.current;
      const retryDocument = mergeInkMarks(retry, heldRef.current);
      const retryView = { ...fresh, document: retryDocument };
      viewRef.current = retryView;
      setView(retryView);
      const retried = await commit(retryDocument, fresh.version);
      if (!retried && commitErrorRef.current) setNotice(screenshotErrorText(t, commitErrorRef.current));
      return retried ?? viewRef.current;
    });
    editQueue.current = run.then(() => undefined, () => undefined);
    return run;
  }

  async function commit(document: EditorDocument, baseVersion: number) {
    try {
      const received = await screenshotEditorCommit(baseVersion, document);
      commitErrorRef.current = null;
      const shown = showDocument(received, document);
      setNotice("");
      return shown;
    } catch (caught) {
      commitErrorRef.current = toScreenshotError(caught);
      return null;
    }
  }

  function clearDraft() {
    draftRef.current = null;
    setDraft(null);
  }

  async function commitDraft(snapshot: Draft | null = draftRef.current) {
    const stillCurrent = snapshot != null && draftRef.current?.id === snapshot.id;
    if (!snapshot || !snapshot.text.trim()) {
      if (stillCurrent) clearDraft();
      return viewRef.current;
    }
    const mark = draftToMark(snapshot, textFontRef.current, stylesRef.current.text, stylesRef.current.tag, imageRef.current);
    holdMark(mark);
    if (stillCurrent) clearDraft();
    const base = viewRef.current;
    if (base && base.document.monitorIndex === index && !base.document.marks.some((item) => sameInkMark(item, mark)) && base.document.marks.length < 300) {
      const optimistic = { ...base, document: { ...base.document, marks: [...base.document.marks, mark] } };
      viewRef.current = optimistic;
      setView(optimistic);
    }
    return edit((document) => {
      if (document.monitorIndex !== index) return null;
      if (document.marks.some((item) => sameInkMark(item, mark))) return { ...document, marks: [...document.marks] };
      if (document.marks.length >= 300) return null;
      return { ...document, marks: [...document.marks, mark] };
    });
  }

  function commitWatermark(next: Watermark) {
    const watermarkValue = watermarkPayload(next);
    return edit((document) => {
      if (!document.selection || document.monitorIndex !== index) return null;
      const current = document.watermark;
      if (current?.text === watermarkValue?.text && current?.color === watermarkValue?.color && current?.opacity === watermarkValue?.opacity) return null;
      if (!current && !watermarkValue) return null;
      return { ...document, watermark: watermarkValue };
    });
  }

  function openDraft(next: Draft, event: React.PointerEvent<HTMLElement>) {
    event.preventDefault();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    draggingRef.current = false;
    const previous = draftRef.current;
    const created = { ...next, id: ++draftIdRef.current };
    armingRef.current = true;
    draftRef.current = created;
    setDraft(created);
    if (previous && previous.text.trim()) void commitDraft(previous);
  }

  function textMinimumHeight() {
    return textFontRef.current * 1.4 + 8;
  }

  function cancelFrame() {
    if (frameJob.current) cancelAnimationFrame(frameJob.current);
    frameJob.current = 0;
    frameTask.current = null;
  }

  function enqueueFrame(task: () => void) {
    frameTask.current = task;
    if (frameJob.current) return;
    frameJob.current = requestAnimationFrame(() => {
      frameJob.current = 0;
      const next = frameTask.current;
      frameTask.current = null;
      next?.();
    });
  }

  function hideMagnifier() {
    pendingPoint.current = null;
    if (pointerJob.current) cancelAnimationFrame(pointerJob.current);
    pointerJob.current = 0;
    if (magnifierBoxRef.current) magnifierBoxRef.current.style.display = "none";
  }

  function showMagnifier(point: Point) {
    pendingPoint.current = point;
    if (pointerJob.current) return;
    pointerJob.current = requestAnimationFrame(() => {
      pointerJob.current = 0;
      const next = pendingPoint.current;
      const currentImage = imageRef.current;
      const box = magnifierBoxRef.current;
      const loupe = magnifierRef.current;
      if (!next || !currentImage || !box || !loupe || draftRef.current || gestureRef.current) {
        if (box) box.style.display = "none";
        return;
      }
      const ctx = loupe.getContext("2d");
      if (!ctx) return;
      ctx.imageSmoothingEnabled = false;
      ctx.putImageData(magnifierPatch(currentImage, next.x, next.y), 0, 0);
      const factor = scaleRef.current || 1;
      const place = placeMagnifier({ x: next.x / factor, y: next.y / factor }, { x: 0, y: 0, w: currentImage.width / factor, h: currentImage.height / factor }, { w: 128, h: 150 });
      box.style.display = "block";
      box.style.left = `${place.x}px`;
      box.style.top = `${place.y}px`;
      if (colorRef.current) colorRef.current.textContent = pixelHex(currentImage.data, currentImage.width, currentImage.height, next.x, next.y);
    });
  }

  function sameRect(current: Rect | null, next: Rect | null) {
    if (current === next) return true;
    if (!current || !next) return false;
    return current.x === next.x && current.y === next.y && current.w === next.w && current.h === next.h;
  }

  function pointOf(event: { clientX: number; clientY: number }) {
    const canvas = canvasRef.current;
    const current = imageRef.current;
    if (!canvas || !current) return null;
    const rect = canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    return imagePoint(event.clientX, event.clientY, rect, current.width, current.height);
  }

  function onPointerDown(event: React.PointerEvent<HTMLElement>) {
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest("[data-shot-ui]")) return;
    if (pinnedRef.current) {
      if (event.button !== 0) return;
      if (pinScaleRef.current > 1.01) {
        startPinPan(event);
        return;
      }
      startPinDrag(event);
      return;
    }
    if (countingRef.current) return;
    if (event.button !== 0) return;
    const activeTool = shotModeRef.current === "capture" ? toolRef.current : "adjust";
    const onEditor = Boolean(target?.closest("textarea, input, select, button"));
    if ((activeTool === "text" || activeTool === "tag") && onEditor) return;
    if (activeTool !== "text" && activeTool !== "tag") {
      event.preventDefault();
      event.stopPropagation();
      const field = draftFieldRef.current;
      if (field && document.activeElement === field) {
        armingRef.current = false;
        field.blur();
      }
    }
    const point = pointOf(event);
    const base = viewRef.current;
    const currentImage = imageRef.current;
    if (!point || !base || !currentImage || index == null) return;
    hideMagnifier();
    const selection = base.document.monitorIndex === index ? base.document.selection : null;
    const active = shotModeRef.current === "capture" ? toolRef.current : "adjust";
    const placingText = Boolean(selection && active === "text" && contains(selection, point.x, point.y));
    const placingTag = Boolean(selection && active === "tag" && contains(selection, point.x, point.y));
    const openDraftBox = draftRef.current;
    if (openDraftBox?.kind === "text" && active === "text" && point.x >= openDraftBox.x && point.y >= openDraftBox.y && point.x <= openDraftBox.x + openDraftBox.w && point.y <= openDraftBox.y + openDraftBox.h) {
      draftFieldRef.current?.focus({ preventScroll: true });
      return;
    }
    if (openDraftBox?.kind === "tag" && active === "tag" && Math.hypot(point.x - openDraftBox.x, point.y - openDraftBox.y) < 80) {
      draftFieldRef.current?.focus({ preventScroll: true });
      return;
    }
    if (openDraftBox && !placingText && !placingTag) void commitDraft();
    if (placingTag && selection) {
      openDraft({ id: 0, kind: "tag", x: point.x, y: point.y, text: "", side: "right" }, event);
      return;
    }
    if (placingText) {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      draggingRef.current = true;
      gestureRef.current = { kind: "textbox", start: point };
      return;
    }
    if (active === "watermark") return;
    event.currentTarget.setPointerCapture(event.pointerId);
    draggingRef.current = true;
    const handle = selection && active === "adjust" ? hitHandle(point.x, point.y, selection, scale) : null;
    if (selection && handle) {
      gestureRef.current = { kind: "resize", handle, origin: selection };
      return;
    }
    if (selection && active === "adjust" && contains(selection, point.x, point.y)) {
      gestureRef.current = { kind: "move", start: point, origin: selection };
      return;
    }
    if (selection && active !== "adjust" && contains(selection, point.x, point.y)) {
      gestureRef.current = active === "pen" || active === "highlight" || (active === "mosaic" && mosaicRef.current === "brush")
        ? { kind: "stroke", start: point, points: [] }
        : { kind: "shape", start: point };
      return;
    }
    gestureRef.current = { kind: "select", start: point };
  }

  function onPointerMove(event: React.PointerEvent<HTMLElement>) {
    if (pinnedRef.current) return;
    const point = pointOf(event);
    const currentImage = imageRef.current;
    const base = viewRef.current;
    if (!point || !currentImage || !base || index == null) return;
    if (shotModeRef.current === "capture" && !gestureRef.current && !draftRef.current) showMagnifier(point);
    const gesture = gestureRef.current;
    if (gesture?.kind === "textbox") {
      if (isClick(point.x - gesture.start.x, point.y - gesture.start.y)) {
        cancelFrame();
        setTextFrame(null);
        return;
      }
      enqueueFrame(() => setTextFrame(fittedBox(gesture.start, point, currentImage.width, currentImage.height, MIN_TEXT_BOX, textMinimumHeight())));
      return;
    }
    if (!gesture) {
      const selection = base.document.monitorIndex === index ? base.document.selection : null;
      if (selection && contains(selection, point.x, point.y)) {
        setHover((current) => current === null ? current : null);
        return;
      }
      const monitor = base.monitors[index];
      if (!monitor) return;
      const local = base.windows.map((window) => windowLocal(window, monitor));
      const hit = topWindow(point.x, point.y, local);
      const next = hit ? clampSelection(hit, currentImage.width, currentImage.height) : null;
      setHover((current) => sameRect(current, next) ? current : next);
      return;
    }
    const dx = point.x - ("start" in gesture ? gesture.start.x : gesture.origin.x);
    const dy = point.y - ("start" in gesture ? gesture.start.y : gesture.origin.y);
    if (gesture.kind === "select") {
      if (isClick(point.x - gesture.start.x, point.y - gesture.start.y)) return;
      enqueueFrame(() => setPreviewSelection(clampSelection({ x: Math.min(gesture.start.x, point.x), y: Math.min(gesture.start.y, point.y), w: Math.abs(point.x - gesture.start.x), h: Math.abs(point.y - gesture.start.y) }, currentImage.width, currentImage.height)));
      return;
    }
    if (gesture.kind === "move") {
      if (isClick(dx, dy)) return;
      enqueueFrame(() => setPreviewSelection(moveSelection(gesture.origin, point.x - gesture.start.x, point.y - gesture.start.y, currentImage.width, currentImage.height)));
      return;
    }
    if (gesture.kind === "resize") {
      enqueueFrame(() => setPreviewSelection(resizeSelection(gesture.origin, gesture.handle, point.x, point.y, currentImage.width, currentImage.height)));
      return;
    }
    const selection = base.document.monitorIndex === index ? base.document.selection : null;
    if (!selection) return;
    if (gesture.kind === "shape") {
      enqueueFrame(() => setPreviewMark(shapeMark(toolRef.current, gesture.start, point, selection, stylesRef.current[toolRef.current], currentImage.width, currentImage.height)));
      return;
    }
    const clamped = clampPoint(point, selection);
    if (gesture.points.length === 0 && isClick(clamped.x - gesture.start.x, clamped.y - gesture.start.y)) return;
    const points = (gesture.points.length === 0 ? [clampPoint(gesture.start, selection), clamped] : [...gesture.points, clamped]).slice(0, 8000);
    gesture.points = points;
    enqueueFrame(() => setPreviewMark(strokeMark(toolRef.current, points, stylesRef.current[toolRef.current])));
  }

  async function onPointerUp(event: React.PointerEvent<HTMLElement>) {
    if (pinnedRef.current) return;
    const gesture = gestureRef.current;
    gestureRef.current = null;
    draggingRef.current = false;
    cancelFrame();
    const point = pointOf(event);
    const base = viewRef.current;
    const currentImage = imageRef.current;
    const replacing = gesture?.kind === "select";
    setPreviewSelection(null);
    setPreviewMark(null);
    setTextFrame(null);
    if (!replacing) setClearMarks(false);
    setHover(null);
    if (!gesture || !point || !base || !currentImage || index == null) {
      setClearMarks(false);
      return;
    }
    if (gesture.kind === "select") {
      if (isClick(point.x - gesture.start.x, point.y - gesture.start.y)) {
        const monitor = base.monitors[index];
        if (!monitor) {
          setClearMarks(false);
          return;
        }
        const local = base.windows.map((window) => windowLocal(window, monitor));
        const hit = topWindow(gesture.start.x, gesture.start.y, local);
        const selection = hit ? clampSelection(hit, currentImage.width, currentImage.height) : { x: 0, y: 0, w: currentImage.width, h: currentImage.height };
        if (selection) await edit((document) => ({ ...document, monitorIndex: index, selection }));
        setClearMarks(false);
        setTool("adjust");
        if (hit && shotModeRef.current === "scroll") await scrollCapture();
        return;
      }
      const selection = clampSelection({ x: Math.min(gesture.start.x, point.x), y: Math.min(gesture.start.y, point.y), w: Math.abs(point.x - gesture.start.x), h: Math.abs(point.y - gesture.start.y) }, currentImage.width, currentImage.height);
      if (selection && selection.w >= 1 && selection.h >= 1) {
        await edit((document) => ({ ...document, monitorIndex: index, selection }));
        setTool("adjust");
        if (shotModeRef.current === "scroll") await scrollCapture();
      }
      setClearMarks(false);
      return;
    }
    if (gesture.kind === "textbox") {
      const minH = textMinimumHeight();
      const selection = base.document.monitorIndex === index ? base.document.selection : null;
      const end = !selection || isClick(point.x - gesture.start.x, point.y - gesture.start.y)
        ? { x: gesture.start.x + (selection ? textBoxWidth(gesture.start.x, selection) : MIN_TEXT_BOX), y: gesture.start.y + minH }
        : point;
      const frame = fittedBox(gesture.start, end, currentImage.width, currentImage.height, MIN_TEXT_BOX, minH);
      if (frame) openDraft({ id: 0, kind: "text", x: frame.x, y: frame.y, w: frame.w, h: frame.h, text: "" }, event);
      return;
    }
    if (gesture.kind === "move") {
      if (isClick(point.x - gesture.start.x, point.y - gesture.start.y)) return;
      const selection = moveSelection(gesture.origin, point.x - gesture.start.x, point.y - gesture.start.y, currentImage.width, currentImage.height);
      await edit((document) => ({ ...document, monitorIndex: index, selection }));
      return;
    }
    if (gesture.kind === "resize") {
      const selection = resizeSelection(gesture.origin, gesture.handle, point.x, point.y, currentImage.width, currentImage.height);
      if (selection) await edit((document) => ({ ...document, monitorIndex: index, selection }));
      return;
    }
    const selection = base.document.monitorIndex === index ? base.document.selection : null;
    if (!selection) return;
    const mark = gesture.kind === "shape"
      ? shapeMark(toolRef.current, gesture.start, point, selection, stylesRef.current[toolRef.current], currentImage.width, currentImage.height)
      : gesture.points.length >= 2 ? strokeMark(toolRef.current, gesture.points, stylesRef.current[toolRef.current]) : null;
    if (!mark) return;
    await edit((document) => document.marks.length >= 300 ? null : { ...document, marks: [...document.marks, mark] });
  }

  async function undo() {
    if (draftRef.current?.text.trim()) {
      await commitDraft();
      return;
    }
    draftRef.current = null;
    setDraft(null);
    try {
      remember(await screenshotEditorUndo());
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  async function redo() {
    if (draftRef.current) return;
    try {
      remember(await screenshotEditorRedo());
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  function remember(next: SessionView) {
    showDocument(next);
    const editingWatermark = document.activeElement instanceof HTMLElement && document.activeElement.dataset.shotField === "watermark";
    if (!editingWatermark) setWatermark(next.document.watermark ?? EMPTY_WATERMARK);
  }

  async function currentEditor() {
    if (draftRef.current) {
      if (!draftRef.current.text.trim()) {
        draftRef.current = null;
        setDraft(null);
      } else {
        const next = await commitDraft();
        if (!next?.document.selection || next.document.monitorIndex !== index) return null;
      }
    }
    const base = await commitWatermark(watermarkRef.current);
    if (!base?.document.selection || base.document.monitorIndex !== index) return null;
    return base;
  }

  function cancelCapture() {
    ocrToken.current += 1;
    setOcrText(null);
    void screenshotScrollStop().catch(() => undefined);
    void screenshotCancel().catch(() => undefined);
  }

  async function scrollCapture() {
    if (scrollingRef.current || index == null) return;
    const base = await currentEditor();
    const selection = base?.document.selection;
    if (!base || !selection || base.document.monitorIndex !== index || selection.w < 32 || selection.h < 48) {
      setNotice(t("zdx.text.screenshot.scrollNeedSelection"));
      return;
    }
    scrollingRef.current = true;
    setScrolling(true);
    setOcrText(null);
    setNotice("");
    try {
      await screenshotScroll(index, selection.x, selection.y, selection.w, selection.h);
    } catch (caught) {
      const error = toScreenshotError(caught);
      if (error.code !== "cancelled") setNotice(screenshotErrorText(t, error));
    } finally {
      scrollingRef.current = false;
      setScrolling(false);
    }
  }

  async function copyRecognizedText() {
    const text = ocrText;
    if (!text || copyingOcrRef.current) return;
    copyingOcrRef.current = true;
    try {
      await screenshotCopyText(text);
      setNotice(t("zdx.text.screenshot.ocrCopied"));
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    } finally {
      copyingOcrRef.current = false;
    }
  }

  async function readText() {
    if (readingRef.current) return;
    const token = ocrToken.current;
    readingRef.current = true;
    setReading(true);
    setOcrText(null);
    try {
      const currentImage = imageRef.current;
      const base = await currentEditor();
      if (token !== ocrToken.current) return;
      if (!currentImage || !base) {
        setNotice(t("zdx.text.screenshot.ocrNeedSelection"));
        return;
      }
      setNotice(t("zdx.text.screenshot.ocrReading"));
      const exported = await selectionPng(currentImage, base.document, true);
      if (token !== ocrToken.current) return;
      const recognized = await screenshotRecognizeText(exported.png);
      if (token !== ocrToken.current) return;
      setNotice("");
      setOcrText(recognized.text);
    } catch (caught) {
      if (token !== ocrToken.current) return;
      const error = toScreenshotError(caught);
      if (error.code === "cancelled") return;
      setNotice(screenshotErrorText(t, error));
    } finally {
      readingRef.current = false;
      setReading(false);
    }
  }

  async function copyImage() {
    if (copyingRef.current) return;
    const currentImage = imageRef.current;
    const base = await currentEditor();
    if (!currentImage || !base) return;
    copyingRef.current = true;
    try {
      const exported = await selectionPng(currentImage, base.document);
      const result = await screenshotCopy(exported.png);
      if (result.history === "skipped_too_large") {
        setNotice(t("zdx.text.screenshot.copiedNoHistory"));
        window.setTimeout(() => void screenshotFinish().catch(() => undefined), 1200);
        return;
      }
      await screenshotFinish();
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    } finally {
      copyingRef.current = false;
    }
  }

  async function saveImage() {
    const currentImage = imageRef.current;
    const base = await currentEditor();
    if (!currentImage || !base) return;
    try {
      const exported = await selectionPng(currentImage, base.document);
      const saved = await screenshotSave(exported.png);
      setNotice(t("zdx.text.screenshot.saved", { path: saved.path }));
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  async function pinImage() {
    if (pinningRef.current || pinnedRef.current || index == null) return;
    const currentImage = imageRef.current;
    if (!currentImage) return;
    pinningRef.current = true;
    try {
      const base = await currentEditor();
      const imageNow = imageRef.current;
      const monitor = base?.monitors[index];
      const selection = base?.document.selection;
      if (!base || !imageNow || !monitor || !selection) return;
      const scale = monitor.scaleFactor || 1;
      const crop = { x: selection.x / scale, y: selection.y / scale, w: selection.w / scale, h: selection.h / scale };
      if (rootRef.current) rootRef.current.style.transform = `translate(${-crop.x}px, ${-crop.y}px)`;
      pinnedRef.current = true;
      await screenshotAdoptPin(
        monitor.x + selection.x,
        monitor.y + selection.y,
        selection.w,
        selection.h,
        scale,
        crop.x,
        crop.y,
        imageNow.width / scale,
        imageNow.height / scale,
      );
      setPinCrop(crop);
      setPinned(true);
      applyPinZoom(1, { x: 0, y: 0 });
      await screenshotFinish();
      await getCurrentWindow().setFocus().catch(() => undefined);
      const document = base.document;
      pngTask.current = selectionPng(imageNow, document).then((exported) => exported.png);
    } catch (caught) {
      pinnedRef.current = false;
      setPinCrop(null);
      setPinned(false);
      applyPinZoom(1, { x: 0, y: 0 });
      if (rootRef.current) rootRef.current.style.transform = "";
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    } finally {
      pinningRef.current = false;
    }
  }

  async function copyPinned() {
    try {
      const png = await pngTask.current;
      if (!png) return;
      const result = await screenshotCopy(png);
      if (result.history === "skipped_too_large") setNotice(t("zdx.text.screenshot.copiedNoHistory"));
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  async function releasePin() {
    pinnedRef.current = false;
    try {
      await screenshotReleasePin();
    } catch (caught) {
      pinnedRef.current = true;
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
      return;
    }
    setPinCrop(null);
    setPinned(false);
    applyPinZoom(1, { x: 0, y: 0 });
    pngTask.current = null;
    if (rootRef.current) rootRef.current.style.transform = "";
  }

  function startPinDrag(event: React.PointerEvent<HTMLElement>) {
    event.preventDefault();
    const surface = event.currentTarget;
    surface.setPointerCapture(event.pointerId);
    const origin = { x: event.screenX, y: event.screenY };
    const win = getCurrentWindow();
    let pointer = origin;
    let start = { x: 0, y: 0 };
    let ready = false;
    let stopped = false;
    let sending = false;
    let latest: { x: number; y: number } | null = null;
    const send = () => {
      if (sending || !latest || stopped) return;
      sending = true;
      const next = latest;
      latest = null;
      void win.setPosition(new LogicalPosition(next.x, next.y)).catch(() => {
        if (!stopped) setNotice(t("zdx.text.screenshot.error.unknown"));
      }).finally(() => {
        sending = false;
        if (!stopped) send();
      });
    };
    const move = (ev: PointerEvent) => {
      pointer = { x: ev.screenX, y: ev.screenY };
      if (!ready) return;
      latest = draggedOrigin(start, origin, pointer);
      send();
    };
    const end = () => {
      if (!stopped && ready && latest) {
        const next = latest;
        latest = null;
        void win.setPosition(new LogicalPosition(next.x, next.y)).catch(() => undefined);
      }
      stopped = true;
      surface.removeEventListener("pointermove", move);
      surface.removeEventListener("pointerup", end);
      surface.removeEventListener("pointercancel", end);
      if (surface.hasPointerCapture(event.pointerId)) surface.releasePointerCapture(event.pointerId);
    };
    surface.addEventListener("pointermove", move);
    surface.addEventListener("pointerup", end);
    surface.addEventListener("pointercancel", end);
    void Promise.all([win.outerPosition(), win.scaleFactor()]).then(([position, factor]) => {
      if (stopped) return;
      start = position.toLogical(factor || 1);
      ready = true;
      if (pointer.x === origin.x && pointer.y === origin.y) return;
      latest = draggedOrigin(start, origin, pointer);
      send();
    }).catch(() => {
      if (!stopped) setNotice(t("zdx.text.screenshot.error.unknown"));
      end();
    });
  }

  function startPinPan(event: React.PointerEvent<HTMLElement>) {
    event.preventDefault();
    const surface = event.currentTarget;
    surface.setPointerCapture(event.pointerId);
    const origin = { x: event.clientX, y: event.clientY };
    const start = { ...pinPanRef.current };
    const crop = pinCropRef.current;
    const scale = pinScaleRef.current;
    let stopped = false;
    const move = (ev: PointerEvent) => {
      if (stopped || !crop) return;
      applyPinZoom(scale, clampPan(
        { x: start.x + ev.clientX - origin.x, y: start.y + ev.clientY - origin.y },
        scale,
        crop.w,
        crop.h,
      ));
    };
    const end = () => {
      stopped = true;
      setPinPanning(false);
      surface.removeEventListener("pointermove", move);
      surface.removeEventListener("pointerup", end);
      surface.removeEventListener("pointercancel", end);
      if (surface.hasPointerCapture(event.pointerId)) surface.releasePointerCapture(event.pointerId);
    };
    surface.addEventListener("pointermove", move);
    surface.addEventListener("pointerup", end);
    surface.addEventListener("pointercancel", end);
    setPinPanning(true);
  }

  function onTextResizeDown(handle: Handle, event: React.PointerEvent<HTMLButtonElement>) {
    const current = draftRef.current;
    if (current?.kind !== "text") return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    textResizeRef.current = { handle, origin: { x: current.x, y: current.y, w: current.w, h: current.h } };
  }

  function onTextResizeMove(event: React.PointerEvent<HTMLButtonElement>) {
    const resize = textResizeRef.current;
    const image = imageRef.current;
    const current = draftRef.current;
    if (!resize || !image || current?.kind !== "text") return;
    const point = pointOf(event);
    if (!point) return;
    const next = resizeTextBox(resize.origin, resize.handle, point, image.width, image.height, MIN_TEXT_BOX, textMinimumHeight());
    if (!next) return;
    const updated = { ...current, x: next.x, y: next.y, w: next.w, h: next.h };
    draftRef.current = updated;
    setDraft(updated);
  }

  function onTextResizeUp() {
    textResizeRef.current = null;
    requestAnimationFrame(() => {
      const field = draftFieldRef.current;
      const current = draftRef.current;
      if (!field || current?.kind !== "text") return;
      const overflow = field.scrollHeight - field.clientHeight;
      if (overflow <= 1) return;
      const next = { ...current, h: current.h + overflow * scale };
      draftRef.current = next;
      setDraft(next);
    });
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const target = event.target as HTMLElement;
    const editing = target.tagName === "TEXTAREA" || target.tagName === "INPUT";
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z" && !editing) {
      event.preventDefault();
      if (event.shiftKey) void redo();
      else void undo();
      return;
    }
    if (pinnedRef.current && !editing && (event.key === "+" || event.key === "=")) {
      event.preventDefault();
      zoomPinBy(1.25);
      return;
    }
    if (pinnedRef.current && !editing && (event.key === "-" || event.key === "_")) {
      event.preventDefault();
      zoomPinBy(1 / 1.25);
      return;
    }
    if (pinnedRef.current && !editing && event.key === "0" && !event.metaKey && !event.ctrlKey) {
      event.preventDefault();
      applyPinZoom(1, { x: 0, y: 0 });
      return;
    }
    if (pinnedRef.current && event.key === "Escape" && !editing) {
      event.preventDefault();
      void releasePin();
      return;
    }
    if (event.key === "Escape" && !editing) {
      event.preventDefault();
      if (countingRef.current) {
        countdownToken.current += 1;
        countingRef.current = false;
        setCountdown(null);
        return;
      }
      if (gestureRef.current) {
        gestureRef.current = null;
        draggingRef.current = false;
        setPreviewSelection(null);
        setPreviewMark(null);
        setTextFrame(null);
        setClearMarks(false);
        return;
      }
      if (scrollingRef.current) {
        void screenshotScrollStop();
        return;
      }
      cancelCapture();
      return;
    }
    if (event.key === "Enter" && !editing && !gestureRef.current && ocrText && !(target.closest("[data-ocr-dialog]"))) {
      event.preventDefault();
      void copyRecognizedText();
      return;
    }
    if (event.key === "Enter" && !editing && !gestureRef.current && shown && shotModeRef.current === "capture") {
      event.preventDefault();
      void copyImage();
    }
  }

  function chooseShotMode(mode: "capture" | "record" | "scroll") {
    if (countingRef.current) return;
    shotModeRef.current = mode;
    setShotMode(mode);
    if (mode !== "capture") {
      hideMagnifier();
      chooseTool("adjust");
    }
  }

  async function beginRecording(format: "mp4" | "gif") {
    if (countingRef.current || shotModeRef.current !== "record" || index == null) return;
    const token = countdownToken.current + 1;
    countdownToken.current = token;
    countingRef.current = true;
    try {
      for (const step of [3, 2, 1]) {
        if (countdownToken.current !== token) return;
        setCountdown(step);
        await new Promise((resolve) => window.setTimeout(resolve, 800));
      }
      if (countdownToken.current !== token) return;
      const base = viewRef.current;
      const selection = base && base.document.monitorIndex === index ? base.document.selection : null;
      if (!selection || selection.w < 8 || selection.h < 8) return;
      await screenshotRecordStart(index, selection.x, selection.y, selection.w, selection.h, format);
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    } finally {
      if (countdownToken.current === token) {
        countingRef.current = false;
        setCountdown(null);
      }
    }
  }

  function chooseTool(item: ShotTool) {
    const next = toolRef.current === item ? "adjust" : item;
    armingRef.current = false;
    const field = draftFieldRef.current;
    if (field && document.activeElement === field) field.blur();
    const draft = draftRef.current;
    if (draft && next !== draft.kind) void commitDraft();
    toolRef.current = next;
    setTool(next);
    rootRef.current?.focus({ preventScroll: true });
    window.scrollTo(0, 0);
  }

  function updateStyle(next: Partial<Style>) {
    setStyles((current) => ({ ...current, [tool]: { ...current[tool], ...next } }));
  }

  function flipTag() {
    if (draft?.kind === "tag") {
      const next = { ...draft, side: draft.side === "left" ? "right" as const : "left" as const };
      draftRef.current = next;
      setDraft(next);
      return;
    }
    const base = viewRef.current;
    if (!base) return;
    void edit((document) => {
      const marks = [...document.marks];
      const last = marks[marks.length - 1];
      if (!last || last.kind !== "tag") return null;
      marks[marks.length - 1] = { ...last, side: last.side === "left" ? "right" : "left" };
      return { ...document, marks };
    });
  }

  if (index == null) return <OverlayMessage text={t("zdx.text.screenshot.error.capture_failed")} action={t("zdx.label.screenshot.cancel")} onAction={() => cancelCapture()} />;
  if (error) return <OverlayMessage text={error} action={t("zdx.label.screenshot.cancel")} onAction={() => cancelCapture()} />;

  const frameWidth = (image?.width ?? monitor?.width ?? 0) / scale;
  const frameHeight = (image?.height ?? monitor?.height ?? 0) / scale;
  const monitorWidth = (monitor?.width ?? image?.width ?? 0) / scale;
  const monitorHeight = (monitor?.height ?? image?.height ?? 0) / scale;
  const fit = frameWidth > 0 && frameHeight > 0 && monitorWidth > 0 && monitorHeight > 0 ? Math.min(1, monitorWidth / frameWidth, monitorHeight / frameHeight) : 1;
  const workspace = { x: 0, y: 0, w: frameWidth, h: frameHeight };
  const logical = shown ? { x: shown.x / scale, y: shown.y / scale, w: shown.w / scale, h: shown.h / scale } : null;
  const toolbar = logical ? placeToolbar(logical, toolbarSize, workspace) : null;
  const property = toolbar && DRAW_TOOLS.includes(tool) ? placePropertyBar({ x: toolbar.x, y: toolbar.y, h: toolbarSize.h }, propertySize, toolbar.side) : null;
  const recordSelection = view?.document.monitorIndex === index ? view.document.selection : null;
  const recordReady = shotMode === "record" && countdown == null && previewSelection == null && Boolean(recordSelection && recordSelection.w >= 8 && recordSelection.h >= 8);
  const recordLogical = recordReady && recordSelection ? { x: recordSelection.x / scale, y: recordSelection.y / scale, w: recordSelection.w / scale, h: recordSelection.h / scale } : null;
  const recordToolbar = recordLogical ? placeToolbar(recordLogical, recordBarSize, workspace) : null;
  const label = shown ? t("zdx.text.screenshot.size", { width: Math.round(shown.w), height: Math.round(shown.h) }) : "";
  const labelBox = logical ? placeSizeLabel(logical, { w: Math.max(72, label.length * 8), h: 22 }, workspace) : null;

  return (
    <div
      ref={rootRef}
      tabIndex={0}
      role="application"
      aria-label={t("zdx.label.screenshot.title")}
      className="relative overflow-hidden bg-black text-white outline-none select-none"
      style={{ width: fit < 1 ? monitorWidth : frameWidth, height: fit < 1 ? monitorHeight : frameHeight, transform: pinCrop ? `translate(${-pinCrop.x}px, ${-pinCrop.y}px)` : undefined, transformOrigin: "top left" }}
      onKeyDown={onKeyDown}
      onDoubleClick={(event) => {
        if (!pinnedRef.current || pinScaleRef.current <= 1.01) return;
        const target = event.target instanceof Element ? event.target : null;
        if (target?.closest("[data-shot-ui]")) return;
        event.preventDefault();
        applyPinZoom(1, { x: 0, y: 0 });
      }}
      onContextMenu={(event) => event.preventDefault()}
      onPointerDownCapture={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={(event) => void onPointerUp(event)}
      onPointerLeave={(event) => {
        if (event.buttons === 0) {
          hideMagnifier();
          setHover(null);
        }
      }}
    >
      <div className={fit < 1 ? "relative" : "contents"} style={fit < 1 ? { width: frameWidth, height: frameHeight, transform: `scale(${fit})`, transformOrigin: "top left" } : undefined}>
      <canvas
        ref={canvasRef}
        width={image?.width ?? monitor?.width ?? 1}
        height={image?.height ?? monitor?.height ?? 1}
        className={pinned ? (pinPanning ? "absolute left-0 top-0 z-0 block cursor-grabbing" : "absolute left-0 top-0 z-0 block cursor-grab") : "absolute left-0 top-0 block"}
        style={{ width: frameWidth, height: frameHeight, ...(pinCrop ? pinnedPictureStyle(pinCrop, pinScale, pinPan) : {}) }}
      />
      {shown && !pinned && tool === "adjust" && handlePoints(shown).map((point) => (
        <span key={point.id} className="pointer-events-none absolute box-border border border-[#3B82F6] bg-white" style={{ left: point.x / scale - 4, top: point.y / scale - 4, width: 8, height: 8 }} />
      ))}
      {!pinned && labelBox && <span className="pointer-events-none absolute rounded bg-black/70 px-1.5 py-0.5 text-xs text-white" style={{ left: labelBox.x, top: labelBox.y }}>{label}</span>}
      <div ref={magnifierBoxRef} className="pointer-events-none absolute overflow-hidden rounded-lg bg-black" style={{ display: "none", width: 128 }}>
        <canvas ref={magnifierRef} width={32} height={32} className="block h-32 w-32" style={{ imageRendering: "pixelated" }} />
        <div ref={colorRef} className="px-2 py-1 text-center text-xs" />
      </div>
      {!pinned && textFrame && (
        <div className="pointer-events-none absolute z-10 outline outline-2 outline-dashed outline-[#3B82F6]" style={{ left: textFrame.x / scale, top: textFrame.y / scale, width: textFrame.w / scale, height: textFrame.h / scale }} />
      )}
      {!pinned && shotMode === "capture" && tool === "text" && draft?.kind === "text" ? (
        <div
          className="absolute z-10 outline outline-2 outline-dashed outline-[#3B82F6]"
          style={{ left: draft.x / scale, top: draft.y / scale, width: draft.w / scale, height: draft.h / scale }}
          onMouseDown={(event) => {
            if ((event.target as HTMLElement).tagName === "TEXTAREA") return;
            event.preventDefault();
          }}
        >
          <textarea
            ref={draftFieldRef}
            autoFocus
            aria-label={t("zdx.label.screenshot.text")}
            value={draft.text}
            maxLength={4000}
            onChange={(event) => {
              const current = draftRef.current;
              if (current?.kind !== "text") return;
              const next = { ...current, text: event.target.value };
              draftRef.current = next;
              setDraft(next);
            }}
            onBlur={(event) => {
              if (armingRef.current) {
                draftFieldRef.current?.focus({ preventScroll: true });
                return;
              }
              const next = event.relatedTarget;
              if (next instanceof Element && next.closest("[data-shot-ui]")) return;
              const latest = draftRef.current;
              if (!latest || latest.id !== draft.id) return;
              void commitDraft(latest);
            }}
            onKeyDown={(event) => {
              if (event.key !== "Escape") return;
              event.preventDefault();
              event.stopPropagation();
              event.currentTarget.blur();
            }}
            className="pointer-events-auto h-full w-full resize-none select-text overflow-hidden border-0 bg-transparent p-0 outline-none"
            style={{
              ...EDITABLE_STYLE,
              color: "transparent",
              WebkitTextFillColor: "transparent",
              caretColor: PALETTE[styles.text.color],
              backgroundColor: "transparent",
              WebkitAppearance: "none",
              appearance: "none",
              fontSize: textFont / scale,
              lineHeight: 1.4,
              fontFamily: CANVAS_FONT_FAMILY,
            }}
          />
          {TEXT_HANDLES.map((handle) => (
            <button
              key={handle}
              type="button"
              aria-label={t("zdx.label.screenshot.resize")}
              className={TEXT_HANDLE_CLASS[handle]}
              style={TEXT_HANDLE_STYLE[handle]}
              onPointerDown={(event) => onTextResizeDown(handle, event)}
              onPointerMove={onTextResizeMove}
              onPointerUp={onTextResizeUp}
            />
          ))}
        </div>
      ) : !pinned && shotMode === "capture" && tool === "tag" && draft?.kind === "tag" ? (
        <textarea
          ref={draftFieldRef}
          autoFocus
          aria-label={t("zdx.label.screenshot.tag")}
          value={draft.text}
          maxLength={4000}
          onChange={(event) => {
            const current = draftRef.current;
            if (current?.kind !== "tag") return;
            const next = { ...current, text: event.target.value };
            draftRef.current = next;
            setDraft(next);
          }}
          onBlur={(event) => {
            if (armingRef.current) {
              draftFieldRef.current?.focus({ preventScroll: true });
              return;
            }
            const next = event.relatedTarget;
            if (next instanceof Element && next.closest("[data-shot-ui]")) return;
            const latest = draftRef.current;
            if (!latest || latest.id !== draft.id) return;
            void commitDraft(latest);
          }}
          onKeyDown={(event) => {
            if (event.key !== "Escape") return;
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.blur();
          }}
          className="pointer-events-auto absolute z-10 resize-none select-text border-2 border-[#3B82F6] bg-transparent p-0 outline-none"
          style={{
            ...EDITABLE_STYLE,
            left: draft.x / scale,
            top: draft.y / scale,
            width: 200 / scale,
            minHeight: (textSize(styles.tag.size) * 1.4) / scale,
            color: PALETTE[styles.tag.color],
            WebkitTextFillColor: PALETTE[styles.tag.color],
            caretColor: PALETTE[styles.tag.color],
            backgroundColor: "transparent",
            fontSize: textSize(styles.tag.size) / scale,
            lineHeight: 1.4,
            fontFamily: CANVAS_FONT_FAMILY,
          }}
        />
      ) : null}
      {recordToolbar && !pinned && (
        <div className="absolute z-10" style={{ left: recordToolbar.x, top: recordToolbar.y }} data-shot-ui="true" onMouseDown={(event) => event.preventDefault()}>
          <div ref={recordBarRef} role="toolbar" aria-label={t("zdx.label.screenshot.modeRecord")} className="flex items-center gap-1 rounded-lg p-1" style={{ height: 40, background: "rgba(28, 28, 28, 0.92)" }}>
            <ChoiceButton label={t("zdx.label.screenshot.recordMp4")} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); void beginRecording("mp4"); }} />
            <ChoiceButton label={t("zdx.label.screenshot.recordGif")} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); void beginRecording("gif"); }} />
            <ChoiceButton label={t("zdx.label.screenshot.cancel")} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); cancelCapture(); }} />
          </div>
        </div>
      )}
      {toolbar && !pinned && shotMode === "capture" && (
        <div className="absolute z-10" style={{ left: toolbar.x, top: toolbar.y }} data-shot-ui="true" onMouseDown={(event) => {
          if (!(event.target as HTMLElement).closest("input, textarea, select")) event.preventDefault();
        }}>
          {property && (
            <div ref={propertyRef} className="absolute flex items-center gap-1 rounded-lg px-1" style={{ left: 0, top: property.y - toolbar.y, height: 36, background: "rgba(28, 28, 28, 0.92)" }}>
              <PropertyBar
                tool={tool}
                style={styles[tool]}
                mosaicMode={mosaicMode}
                watermark={watermark}
                onStyle={updateStyle}
                onMosaicMode={setMosaicMode}
                onFlip={flipTag}
                fontSize={textFont}
                onFontSize={setTextFont}
                onWatermark={(next) => {
                  watermarkRef.current = next;
                  setWatermark(next);
                  setStyles((current) => ({ ...current, watermark: { ...current.watermark, color: next.color } }));
                }}
                onWatermarkCommit={(next) => void commitWatermark(next)}
              />
            </div>
          )}
          <div ref={toolbarRef} role="toolbar" aria-label={t("zdx.label.screenshot.tools")} className="flex items-center gap-1 rounded-lg p-1" style={{ height: 40, background: "rgba(28, 28, 28, 0.92)" }} onKeyDown={moveToolbarFocus}>
            {DRAW_TOOLS.map((item) => (
              <ToolButton key={item} label={t(`zdx.label.screenshot.${item}`)} pressed={tool === item} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); chooseTool(item); }}>
                <ShotGlyph name={item} />
              </ToolButton>
            ))}
            <ToolButton label={t("zdx.label.screenshot.undo")} disabled={!view?.canUndo} onClick={() => void undo()}><ShotGlyph name="undo" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.redo")} disabled={!view?.canRedo} onClick={() => void redo()}><ShotGlyph name="redo" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.scroll")} disabled={!shown || reading || scrolling || shotMode !== "capture"} onClick={() => void scrollCapture()}><ShotGlyph name="scroll" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.ocr")} disabled={reading} onClick={() => void readText()}><ShotGlyph name="ocr" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.pin")} onClick={() => void pinImage()}><ShotGlyph name="pin" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.download")} onClick={() => void saveImage()}><ShotGlyph name="download" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.copy")} onClick={() => void copyImage()}><ShotGlyph name="copy" /></ToolButton>
            <ToolButton label={t("zdx.label.screenshot.cancel")} onClick={() => cancelCapture()}><ShotGlyph name="cancel" /></ToolButton>
          </div>
        </div>
      )}
      {!pinned && (
        <div className="absolute top-12 left-1/2 z-30 flex -translate-x-1/2 flex-col items-center gap-2" data-shot-ui="true" onMouseDown={(event) => event.preventDefault()}>
          <div role="group" aria-label={t("zdx.label.screenshot.mode")} className="flex items-center gap-1 rounded-lg p-1" style={{ background: "rgba(28, 28, 28, 0.92)" }}>
            <ChoiceButton label={t("zdx.label.screenshot.modeCapture")} pressed={shotMode === "capture"} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); chooseShotMode("capture"); }} />
            <ChoiceButton label={t("zdx.label.screenshot.modeRecord")} pressed={shotMode === "record"} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); chooseShotMode("record"); }} />
            <ChoiceButton label={t("zdx.label.screenshot.modeScroll")} pressed={shotMode === "scroll"} onMouseDown={(event) => { event.preventDefault(); event.stopPropagation(); chooseShotMode("scroll"); }} />
          </div>
          {shotMode === "record" && <span className="max-w-md rounded-lg bg-black/80 px-3 py-1 text-center text-xs leading-5 text-white">{t("zdx.text.screenshot.recordHint")}</span>}
          {shotMode === "scroll" && <span className="max-w-md rounded-lg bg-black/80 px-3 py-1 text-center text-xs leading-5 text-white">{t("zdx.text.screenshot.scrollModeHint")}</span>}
        </div>
      )}
      {countdown != null && (
        <div className="pointer-events-none absolute inset-0 z-40 flex items-center justify-center text-7xl font-semibold text-white" aria-live="assertive">{countdown}</div>
      )}
      {ocrText != null && (
        <div
          role="dialog"
          aria-labelledby="ocr-result-title"
          data-shot-ui="true"
          data-ocr-dialog="true"
          className="absolute top-1/2 left-1/2 z-50 flex w-[480px] max-w-[80%] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-xl p-4"
          style={{ background: "rgba(28, 28, 28, 0.96)" }}
          onMouseDown={(event) => {
            const clicked = event.target as HTMLElement;
            if (clicked.closest("[data-ocr-text], button")) return;
            event.preventDefault();
          }}
        >
          <p id="ocr-result-title" className="text-sm font-medium text-white">{t("zdx.label.screenshot.ocrResult")}</p>
          <div data-ocr-text="true" className="max-h-64 overflow-auto text-sm leading-6 break-words whitespace-pre-wrap text-white select-text">{ocrText}</div>
          <div className="flex justify-end gap-2">
            <button type="button" className="h-8 rounded-lg border-0 bg-[#3B82F6] px-3 text-sm text-white" onClick={() => void copyRecognizedText()}>{t("zdx.label.screenshot.ocrCopy")}</button>
            <button type="button" className="h-8 rounded-lg border-0 bg-transparent px-3 text-sm text-white hover:bg-white/15" onClick={() => cancelCapture()}>{t("zdx.label.screenshot.cancel")}</button>
          </div>
        </div>
      )}
      {(notice || (view?.truncated && index === 0)) && (
        <div className="pointer-events-none absolute top-36 left-1/2 z-30 max-w-[80vw] -translate-x-1/2 rounded-lg bg-black/80 px-3 py-2 text-sm text-white">
          {notice || t("zdx.text.screenshot.truncated")}
        </div>
      )}
      {!image && !error && <div className="pointer-events-none absolute inset-0" aria-busy="true" />}
      {pinCrop && (
        <div
          className="absolute z-30 flex cursor-default gap-1 rounded-lg p-1"
          style={{ left: pinCrop.x + Math.max(4, pinCrop.w - 148), top: pinCrop.y + 8, background: "rgba(28, 28, 28, 0.92)" }}
          data-shot-ui="true"
          onMouseDown={(event) => event.preventDefault()}
          onPointerDown={(event) => event.stopPropagation()}
        >
          <ToolButton label={t("zdx.label.screenshot.zoomIn")} disabled={pinScale > 5.99} onClick={() => zoomPinBy(1.25)}><ShotGlyph name="zoomIn" /></ToolButton>
          <ToolButton label={t("zdx.label.screenshot.zoomOut")} disabled={pinScale <= 1.01} onClick={() => zoomPinBy(1 / 1.25)}><ShotGlyph name="zoomOut" /></ToolButton>
          <ToolButton label={t("zdx.label.screenshot.pinCopy")} onClick={() => void copyPinned()}><ShotGlyph name="copy" /></ToolButton>
          <ToolButton label={t("zdx.label.screenshot.pinClose")} onClick={() => void releasePin()}><ShotGlyph name="cancel" /></ToolButton>
        </div>
      )}
      </div>
    </div>
  );
}

function shapeMark(tool: ShotTool, start: Point, end: Point, selection: Rect, style: Style, width: number, height: number): Mark | null {
  if (tool === "arrow") {
    const a = clampPoint(start, selection);
    const b = clampPoint(end, selection);
    if (Math.hypot(b.x - a.x, b.y - a.y) < 2) return null;
    return { kind: "arrow", x1: a.x, y1: a.y, x2: b.x, y2: b.y, size: style.size, color: style.color };
  }
  const rect = boxOf(start, end, selection, width, height);
  if (!rect || rect.w < 2 || rect.h < 2) return null;
  if (tool === "mosaic") return { kind: "mosaicRect", ...rect, size: style.size };
  if (tool === "rect" || tool === "ellipse") return { kind: tool, ...rect, size: style.size, color: style.color };
  return null;
}

function strokeMark(tool: ShotTool, points: Point[], style: Style): Mark | null {
  if (points.length < 2) return null;
  if (tool === "mosaic") return { kind: "mosaicPen", points, size: style.size };
  if (tool === "pen" || tool === "highlight") return { kind: tool, points, size: style.size, color: style.color };
  return null;
}

function parseLoadedFrame(bytes: Uint8Array) {
  try {
    const parsed = parseFrame(bytes);
    return new ImageData(new Uint8ClampedArray(parsed.pixels), parsed.width, parsed.height);
  } catch {
    throw new ScreenshotRequestError("capture_failed");
  }
}

function OverlayMessage({ text, action, onAction }: { text: string; action: string; onAction: () => void }) {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-black text-white">
      <div className="flex max-w-md flex-col items-center gap-3 px-6 text-center">
        <p>{text}</p>
        <button type="button" className="rounded-lg bg-white px-3 py-1.5 text-sm text-black" onClick={onAction}>{action}</button>
      </div>
    </div>
  );
}

function ChoiceButton({
  label,
  pressed = false,
  onMouseDown,
}: {
  label: string;
  pressed?: boolean;
  onMouseDown: (event: React.MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      className={pressed
        ? "h-8 rounded-lg border-0 bg-[#3B82F6] px-3 text-sm text-white"
        : "h-8 rounded-lg border-0 bg-transparent px-3 text-sm text-white hover:bg-white/15"}
      onMouseDown={onMouseDown}
    >
      {label}
    </button>
  );
}

function ToolButton({
  label,
  pressed = false,
  disabled = false,
  title,
  placement = "top",
  onClick,
  onMouseDown,
  children,
}: {
  label: string;
  pressed?: boolean;
  disabled?: boolean;
  title?: string;
  placement?: "top" | "bottom";
  onClick?: () => void;
  onMouseDown?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  children: React.ReactNode;
}) {
  return (
    <Hint title={title ?? label} placement={placement}>
      <span className="inline-flex">
        <button
          type="button"
          aria-label={label}
          aria-pressed={pressed}
          disabled={disabled}
          onClick={onClick}
          onMouseDown={onMouseDown}
          className={pressed
            ? "flex h-8 w-8 items-center justify-center rounded-lg border-0 bg-[#3B82F6] p-0 text-white"
            : disabled
              ? "flex h-8 w-8 cursor-not-allowed items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white opacity-40"
              : "flex h-8 w-8 items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white hover:bg-white/15"}
        >
          {children}
        </button>
      </span>
    </Hint>
  );
}

function Hint({ title, placement = "top", children }: { title: string; placement?: "top" | "bottom"; children: React.ReactElement }) {
  return (
    <Tooltip title={title} placement={placement} mouseEnterDelay={0.2}>
      {children}
    </Tooltip>
  );
}

function moveToolbarFocus(event: React.KeyboardEvent<HTMLDivElement>) {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
  if (current < 0) return;
  event.preventDefault();
  buttons[(current + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
}

function PropertyBar({
  tool,
  style,
  mosaicMode,
  watermark,
  onStyle,
  onMosaicMode,
  onFlip,
  fontSize,
  onFontSize,
  onWatermark,
  onWatermarkCommit,
}: {
  tool: ShotTool;
  style: Style;
  mosaicMode: "brush" | "box";
  watermark: Watermark;
  onStyle: (next: Partial<Style>) => void;
  onMosaicMode: (mode: "brush" | "box") => void;
  onFlip: () => void;
  fontSize: number;
  onFontSize: (size: number) => void;
  onWatermark: (next: Watermark) => void;
  onWatermarkCommit: (next: Watermark) => void;
}) {
  const { t } = useTranslation();
  const shape = tool === "rect" || tool === "ellipse" || tool === "arrow" || tool === "pen" || tool === "highlight";
  return (
    <>
      {tool === "mosaic" && (
        <>
          <ToolButton label={t("zdx.label.screenshot.brush")} pressed={mosaicMode === "brush"} onClick={() => onMosaicMode("brush")}><ShotGlyph name="pen" /></ToolButton>
          <ToolButton label={t("zdx.label.screenshot.box")} pressed={mosaicMode === "box"} onClick={() => onMosaicMode("box")}><ShotGlyph name="rect" /></ToolButton>
        </>
      )}
      {tool === "text" && <FontChoices label={t("zdx.label.screenshot.fontSize")} value={fontSize} onChange={onFontSize} />}
      {tool !== "watermark" && tool !== "text" && (
        <SizeChoices label={t(tool === "tag" ? "zdx.label.screenshot.fontSize" : tool === "mosaic" ? "zdx.label.screenshot.block" : "zdx.label.screenshot.stroke")} value={style.size} onChange={(size) => onStyle({ size })} />
      )}
      {tool === "tag" && <ToolButton label={t("zdx.label.screenshot.flip")} onClick={onFlip}><ShotGlyph name="flip" /></ToolButton>}
      {tool === "watermark" && (
        <>
          <Hint title={t("zdx.label.screenshot.watermark")}>
            <input
              data-shot-field="watermark"
              aria-label={t("zdx.label.screenshot.watermark")}
              placeholder={t("zdx.placeholder.screenshot.watermark")}
              value={watermark.text}
              maxLength={40}
              onChange={(event) => onWatermark({ ...watermark, text: event.target.value })}
              onBlur={(event) => onWatermarkCommit({ ...watermark, text: event.target.value })}
              onKeyDown={(event) => {
                if (event.key !== "Escape" && event.key !== "Enter") return;
                event.preventDefault();
                event.stopPropagation();
                event.currentTarget.blur();
              }}
              className="h-7 w-36 select-text rounded border-0 bg-white/10 px-2 text-sm text-white outline-none"
              style={EDITABLE_STYLE}
            />
          </Hint>
          <Hint title={t("zdx.label.screenshot.opacity")}>
            <input
              aria-label={t("zdx.label.screenshot.opacity")}
              type="range"
              min={0.1}
              max={1}
              step={0.1}
              value={watermark.opacity}
              onChange={(event) => onWatermark({ ...watermark, opacity: Number(event.target.value) })}
              onPointerUp={(event) => onWatermarkCommit({ ...watermark, opacity: Number(event.currentTarget.value) })}
            />
          </Hint>
        </>
      )}
      {(shape || tool === "text" || tool === "tag" || tool === "watermark") && (
        <div role="radiogroup" aria-label={t("zdx.label.screenshot.color")} className="flex items-center gap-1">
          {COLORS.map((color) => (
            <Hint key={color} title={t(`zdx.label.screenshot.color${capitalize(color)}`)}>
              <button
                type="button"
                role="radio"
                aria-label={t(`zdx.label.screenshot.color${capitalize(color)}`)}
                aria-checked={(tool === "watermark" ? watermark.color : style.color) === color}
                onClick={() => {
                  if (tool === "watermark") {
                    const next = { ...watermark, color };
                    onWatermark(next);
                    onWatermarkCommit(next);
                    return;
                  }
                  onStyle({ color });
                }}
                className="h-4 w-4 rounded-full border border-white/40 p-0"
                style={{ background: PALETTE[color] }}
              />
            </Hint>
          ))}
        </div>
      )}
    </>
  );
}

function FontChoices({ label, value, onChange }: { label: string; value: number; onChange: (size: number) => void }) {
  return (
    <Hint title={label}>
      <select
        aria-label={label}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        className="h-7 w-16 rounded border-0 bg-white/10 px-1 text-sm text-white outline-none"
      >
        {TEXT_FONTS.map((size) => (
          <option key={size} value={size}>{size}</option>
        ))}
      </select>
    </Hint>
  );
}

function SizeChoices({ label, value, onChange }: { label: string; value: StrokeSize; onChange: (size: StrokeSize) => void }) {
  const { t } = useTranslation();
  return (
    <div role="radiogroup" aria-label={label} className="flex items-center gap-1">
      {SIZES.map((size) => (
        <Hint key={size} title={t(`zdx.label.screenshot.size${capitalize(size)}`)}>
          <button
            type="button"
            role="radio"
            aria-label={t(`zdx.label.screenshot.size${capitalize(size)}`)}
            aria-checked={value === size}
            onClick={() => onChange(size)}
            className={value === size ? "h-6 rounded bg-[#3B82F6] px-1.5 text-xs text-white" : "h-6 rounded bg-transparent px-1.5 text-xs text-white"}
          >
            {size === "sm" ? "S" : size === "md" ? "M" : "L"}
          </button>
        </Hint>
      ))}
    </div>
  );
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function ShotGlyph({ name }: { name: string }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      {name === "rect" && <rect x="5" y="6" width="14" height="12" rx="1" />}
      {name === "ellipse" && <ellipse cx="12" cy="12" rx="7" ry="5" />}
      {name === "arrow" && <path d="M5 17 L17 7 M12 7 h5 v5" />}
      {name === "pen" && <path d="M4 20 l5-1 L19 9 l-4-4 L5 15 z" />}
      {name === "highlight" && <path d="M5 16 L15 6 l3 3 L8 19 z M4 20 h6" />}
      {name === "mosaic" && <path d="M5 5 h6 v6 H5 z M13 5 h6 v6 h-6 z M5 13 h6 v6 H5 z M13 13 h6 v6 h-6 z" />}
      {name === "text" && <path d="M6 6 h12 M12 6 v12" />}
      {name === "tag" && <path d="M6 8 h8 l4 4 l-6 6 l-4-4 V8 z M9 11 h.01" />}
      {name === "watermark" && <path d="M5 17 l7-10 l7 10 M8 17 h8" />}
      {name === "undo" && <path d="M8 8 H5 v3 M5 9 a7 7 0 1 1 -1 4" />}
      {name === "redo" && <path d="M16 8 h3 v3 M19 9 a7 7 0 1 0 1 4" />}
      {name === "scroll" && <path d="M7 4 h8 a3 3 0 0 1 0 6 H9 a3 3 0 0 0 0 6 h8" />}
      {name === "ocr" && <path d="M6 7 V5 h3 M15 5 h3 v2 M18 15 v2 h-3 M9 17 H6 v-2 M8 9 h8 v6 H8 z" />}
      {name === "pin" && <path d="M9 4 h6 l-1 6 3 2 v2 H7 v-2 l3-2 z M12 14 v6" />}
      {name === "download" && <path d="M12 4 v10 M8 10 l4 4 4-4 M5 19 h14" />}
      {name === "zoomIn" && <path d="M10.5 10.5 m-6.5 0 a6.5 6.5 0 1 0 13 0 a6.5 6.5 0 1 0 -13 0 M15.5 15.5 L20 20 M10.5 7.5 v6 M7.5 10.5 h6" />}
      {name === "zoomOut" && <path d="M10.5 10.5 m-6.5 0 a6.5 6.5 0 1 0 13 0 a6.5 6.5 0 1 0 -13 0 M15.5 15.5 L20 20 M7.5 10.5 h6" />}
      {name === "copy" && <path d="M8 8 h10 v12 H8 z M6 16 H5 V4 h10 v2" />}
      {name === "cancel" && <path d="M7 7 l10 10 M17 7 L7 17" />}
      {name === "flip" && <path d="M7 8 h10 M7 16 h10 M9 5 L6 8 l3 3 M15 13 l3 3 l-3 3" />}
    </svg>
  );
}
