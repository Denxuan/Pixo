import { listen } from "@tauri-apps/api/event";
import { LogicalPosition, getCurrentWindow } from "@tauri-apps/api/window";

import { screenshotCancel, screenshotFinish, screenshotGetSettings, screenshotScrollCopy, screenshotScrollSave, screenshotScrollSetAuto, screenshotScrollStop, toScreenshotError } from "@/api/screenshot";
import { PreviewViewport, ZoomButtons, usePreviewZoom } from "@/components/PreviewZoom";
import { draggedOrigin } from "@/utils/screenshotGeometry";
import { screenshotErrorText } from "@/utils/screenshotMessage";

type ScrollResult = {
  preview: string;
  width: number;
  height: number;
};

export default function ScreenshotScrollHud() {
  const { t } = useTranslation();
  const [height, setHeight] = useState(0);
  const [preview, setPreview] = useState("");
  const [result, setResult] = useState<ScrollResult | null>(null);
  const [notice, setNotice] = useState("");
  const [auto, setAuto] = useState(false);
  const [dragging, setDragging] = useState(false);
  const finished = useRef(false);
  const busy = useRef(false);
  const touchedAuto = useRef(false);
  const generation = useRef(0);
  const zoom = usePreviewZoom();

  useEffect(() => {
    let alive = true;
    let stopBegin: (() => void) | undefined;
    let stopProgress: (() => void) | undefined;
    let stopResult: (() => void) | undefined;
    const accept = (next: number) => {
      if (!Number.isFinite(next)) return false;
      if (generation.current === 0) generation.current = next;
      return next === generation.current;
    };
    void listen<{ generation: number }>("screenshot://scroll-begin", (event) => {
      if (!alive || !Number.isFinite(event.payload.generation)) return;
      if (event.payload.generation < generation.current) return;
      generation.current = event.payload.generation;
      finished.current = false;
      busy.current = false;
      zoom.reset();
      setResult(null);
      setPreview("");
      setHeight(0);
      setNotice("");
    }).then((unlisten) => {
      if (alive) stopBegin = unlisten;
      else unlisten();
    });
    void listen<{ generation: number; height: number; preview?: string | null }>("screenshot://scroll", (event) => {
      if (!alive || !accept(event.payload.generation) || finished.current) return;
      setHeight(event.payload.height);
      if (event.payload.preview) setPreview(`data:image/png;base64,${event.payload.preview}`);
    }).then((unlisten) => {
      if (alive) stopProgress = unlisten;
      else unlisten();
    });
    void listen<{ generation: number; preview?: string | null; width: number; height: number }>("screenshot://scroll-result", (event) => {
      if (!alive || !accept(event.payload.generation)) return;
      finished.current = true;
      zoom.reset();
      setResult({
        preview: event.payload.preview ? `data:image/png;base64,${event.payload.preview}` : "",
        width: event.payload.width,
        height: event.payload.height,
      });
    }).then((unlisten) => {
      if (alive) stopResult = unlisten;
      else unlisten();
    });
    void screenshotGetSettings().then((settings) => {
      if (alive && !touchedAuto.current) setAuto(Boolean(settings.scrollAuto));
    }).catch(() => undefined);
    return () => {
      alive = false;
      stopBegin?.();
      stopProgress?.();
      stopResult?.();
    };
  }, []);

  async function toggleAuto() {
    const next = !auto;
    touchedAuto.current = true;
    setAuto(next);
    try {
      await screenshotScrollSetAuto(next);
    } catch (caught) {
      setAuto(!next);
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  async function copyResult() {
    if (busy.current) return;
    busy.current = true;
    try {
      const copied = await screenshotScrollCopy();
      if (copied.history === "skipped_too_large") {
        setNotice(t("zdx.text.screenshot.copiedNoHistory"));
        window.setTimeout(() => void screenshotFinish().catch(() => undefined), 1200);
        return;
      }
      await screenshotFinish();
    } catch (caught) {
      busy.current = false;
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  async function downloadResult() {
    if (busy.current) return;
    try {
      const saved = await screenshotScrollSave();
      setNotice(t("zdx.text.screenshot.saved", { path: saved.path }));
    } catch (caught) {
      setNotice(screenshotErrorText(t, toScreenshotError(caught)));
    }
  }

  function onPointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
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
      setDragging(false);
      surface.removeEventListener("pointermove", move);
      surface.removeEventListener("pointerup", end);
      surface.removeEventListener("pointercancel", end);
      if (surface.hasPointerCapture(event.pointerId)) surface.releasePointerCapture(event.pointerId);
    };
    surface.addEventListener("pointermove", move);
    surface.addEventListener("pointerup", end);
    surface.addEventListener("pointercancel", end);
    setDragging(true);
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

  const shell = dragging
    ? "flex h-full w-full cursor-grabbing select-none flex-col bg-[#1c1c1c] text-white outline-none"
    : "flex h-full w-full cursor-grab select-none flex-col bg-[#1c1c1c] text-white outline-none";

  if (result) {
    return (
      <div
        className={shell}
        tabIndex={0}
        role="dialog"
        aria-label={t("zdx.label.screenshot.scrollDrag")}
        onPointerDown={onPointerDown}
        onKeyDown={(event) => {
          if (zoom.onKeyDown(event)) return;
          if (event.key === "Escape") {
            event.preventDefault();
            void screenshotCancel();
            return;
          }
          if (event.key === "Enter" && !(event.target instanceof HTMLButtonElement)) {
            event.preventDefault();
            void copyResult();
          }
        }}
      >
        <div className="relative min-h-0 flex-1 overflow-hidden">
          <PreviewViewport zoom={zoom} align="start">
            {result.preview ? (
              <img alt={t("zdx.text.screenshot.scrollResult")} className="max-h-full max-w-full" draggable={false} src={result.preview} />
            ) : (
              <p className="flex h-full items-center justify-center px-4 text-center text-sm">{t("zdx.text.screenshot.scrollResult")}</p>
            )}
          </PreviewViewport>
          <div className="absolute top-2 right-2 flex cursor-default gap-1 rounded-lg p-1" style={{ background: "rgba(28, 28, 28, 0.92)" }}>
            <ZoomButtons zoom={zoom} />
            <IconButton label={t("zdx.label.screenshot.download")} onClick={() => void downloadResult()}>
              <ScrollGlyph name="download" />
            </IconButton>
            <IconButton label={t("zdx.label.screenshot.copy")} autoFocus onClick={() => void copyResult()}>
              <ScrollGlyph name="copy" />
            </IconButton>
            <IconButton label={t("zdx.label.screenshot.cancel")} onClick={() => void screenshotCancel()}>
              <ScrollGlyph name="close" />
            </IconButton>
          </div>
          {notice ? <p className="absolute right-2 bottom-2 left-2 rounded bg-[#1c1c1c]/90 px-2 py-1 text-center text-sm">{notice}</p> : null}
        </div>
        <p className="flex h-10 shrink-0 items-center justify-center px-3 text-sm">{t("zdx.text.screenshot.size", { width: result.width, height: result.height })}</p>
      </div>
    );
  }

  return (
    <div
      className={shell}
      tabIndex={0}
      role="dialog"
      aria-label={t("zdx.label.screenshot.scrollDrag")}
      onPointerDown={onPointerDown}
      onKeyDown={(event) => {
        if (zoom.onKeyDown(event)) return;
        if (event.key !== "Escape") return;
        event.preventDefault();
        void screenshotScrollStop();
      }}
    >
      <div className="relative min-h-0 flex-1 overflow-hidden">
        <PreviewViewport zoom={zoom} align="start">
          <div className="flex h-full w-full items-start justify-center px-2 pt-2">
            {preview ? (
              <img alt={t("zdx.text.screenshot.scrollPreview")} className="max-h-full max-w-full" draggable={false} src={preview} />
            ) : null}
          </div>
        </PreviewViewport>
        <div className="absolute top-2 right-2 flex cursor-default gap-1 rounded-lg p-1" style={{ background: "rgba(28, 28, 28, 0.92)" }}>
          <ZoomButtons zoom={zoom} />
          <IconButton label={t("zdx.label.screenshot.scrollDone")} autoFocus onClick={() => void screenshotScrollStop()}>
            <ScrollGlyph name="check" />
          </IconButton>
        </div>
      </div>
      <p className="shrink-0 px-3 pt-2 text-sm">{auto ? t("zdx.text.screenshot.scrollAutoOn") : t("zdx.text.screenshot.scrollManual")}</p>
      <p className="shrink-0 truncate px-3 py-2 text-sm">{t("zdx.text.screenshot.scrollProgress", { height })}</p>
      <div className="flex shrink-0 items-center justify-between gap-2 border-t border-white/10 px-3 py-2">
        <span className="min-w-0 truncate text-sm">{t("zdx.label.screenshot.scrollAuto")}</span>
        <button
          type="button"
          role="switch"
          aria-checked={auto}
          aria-label={t("zdx.label.screenshot.scrollAuto")}
          className={auto ? "relative h-5 w-9 shrink-0 cursor-default rounded-full border-0 bg-[#3B82F6] p-0" : "relative h-5 w-9 shrink-0 cursor-default rounded-full border-0 bg-white/25 p-0"}
          onClick={() => void toggleAuto()}
        >
          <span className={auto ? "absolute top-0.5 left-4 h-4 w-4 rounded-full bg-white" : "absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white"} />
        </button>
      </div>
      {notice ? <p className="shrink-0 px-3 pb-2 text-center text-sm">{notice}</p> : null}
    </div>
  );
}

function IconButton({
  label,
  autoFocus = false,
  onClick,
  children,
}: {
  label: string;
  autoFocus?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip title={label} placement="bottom" mouseEnterDelay={0.2}>
      <span className="inline-flex">
        <button
          type="button"
          aria-label={label}
          autoFocus={autoFocus}
          onClick={onClick}
          className="flex h-8 w-8 cursor-default items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white hover:bg-white/15"
        >
          {children}
        </button>
      </span>
    </Tooltip>
  );
}

function ScrollGlyph({ name }: { name: "check" | "close" | "copy" | "download" }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      {name === "download" && <path d="M12 4 v10 M8 10 l4 4 4-4 M5 19 h14" />}
      {name === "copy" && <path d="M8 8 h10 v12 H8 z M6 16 H5 V4 h10 v2" />}
      {name === "close" && <path d="M7 7 l10 10 M17 7 L7 17" />}
      {name === "check" && <path d="M5 12 l5 5 L19 7" />}
    </svg>
  );
}
