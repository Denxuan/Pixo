import { listen } from "@tauri-apps/api/event";
import { LogicalPosition, getCurrentWindow } from "@tauri-apps/api/window";

import { screenshotRecordClose, screenshotRecordCopy, screenshotRecordDownload, screenshotRecordPreview, screenshotRecordStop, toScreenshotError } from "@/api/screenshot";
import { PreviewViewport, ZoomButtons, usePreviewZoom } from "@/components/PreviewZoom";
import { draggedOrigin } from "@/utils/screenshotGeometry";
import { screenshotErrorText } from "@/utils/screenshotMessage";

type RecordPhase = "recording" | "exporting" | "saved" | "error";
type RecordFormat = "gif" | "mp4";

type RecordPayload = {
  phase?: RecordPhase;
  format?: RecordFormat;
  code?: string;
  params?: Record<string, unknown>;
};

function isFormat(value: unknown): value is RecordFormat {
  return value === "gif" || value === "mp4";
}

function sniffFormat(bytes: Uint8Array): RecordFormat {
  if (bytes.length >= 3 && bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) return "gif";
  return "mp4";
}

function formatLabel(format: RecordFormat) {
  return format === "mp4" ? "MP4" : "GIF";
}

function formatClock(ms: number) {
  const total = Math.max(0, Math.round(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export default function ScreenshotRecorder() {
  const { t } = useTranslation();
  const tRef = useRef(t);
  tRef.current = t;
  const rootRef = useRef<HTMLDivElement>(null);
  const mediaRef = useRef("");
  const formatRef = useRef<RecordFormat>("gif");
  const videoRef = useRef<HTMLVideoElement>(null);
  const [phase, setPhase] = useState<RecordPhase>("recording");
  const [format, setFormat] = useState<RecordFormat>("gif");
  const [elapsed, setElapsed] = useState(0);
  const [message, setMessage] = useState("");
  const [mediaUrl, setMediaUrl] = useState("");
  const [notice, setNotice] = useState("");
  const [dragging, setDragging] = useState(false);
  const zoom = usePreviewZoom();

  useEffect(() => {
    rootRef.current?.focus();
  }, [phase]);

  useEffect(() => {
    let alive = true;
    let generation = 0;
    let stop: (() => void) | undefined;
    const dropMedia = () => {
      if (mediaRef.current) URL.revokeObjectURL(mediaRef.current);
      mediaRef.current = "";
      setMediaUrl("");
      zoom.reset();
    };
    const showMedia = (token: number, bytes: Uint8Array, kind: RecordFormat) => {
      if (!alive || token !== generation) return;
      const copy = new Uint8Array(bytes);
      const url = URL.createObjectURL(new Blob([copy], { type: kind === "mp4" ? "video/mp4" : "image/gif" }));
      if (mediaRef.current) URL.revokeObjectURL(mediaRef.current);
      mediaRef.current = url;
      formatRef.current = kind;
      setFormat(kind);
      setMediaUrl(url);
      setPhase("saved");
      setNotice("");
      zoom.reset();
    };
    const loadMedia = (token: number, kind: RecordFormat | undefined, reportError: boolean) => {
      void screenshotRecordPreview().then((bytes) => {
        showMedia(token, bytes, kind ?? sniffFormat(bytes));
      }).catch((error) => {
        if (!alive || token !== generation || !reportError) return;
        setNotice(screenshotErrorText(tRef.current, toScreenshotError(error)));
      });
    };
    const rememberFormat = (value: unknown) => {
      if (!isFormat(value)) return;
      formatRef.current = value;
      setFormat(value);
    };
    void listen<RecordPayload>("screenshot://record", (event) => {
      const payload = event.payload;
      if (payload.phase === "recording") {
        generation += 1;
        dropMedia();
        rememberFormat(payload.format);
        setPhase("recording");
        setMessage("");
        setNotice("");
        setElapsed(0);
        return;
      }
      if (payload.phase === "exporting") {
        rememberFormat(payload.format);
        setPhase("exporting");
        return;
      }
      if (payload.phase === "saved") {
        const token = generation;
        rememberFormat(payload.format);
        setPhase("saved");
        setNotice("");
        loadMedia(token, isFormat(payload.format) ? payload.format : formatRef.current, true);
        return;
      }
      if (payload.phase === "error") {
        setPhase("error");
        setMessage(screenshotErrorText(tRef.current, toScreenshotError(payload)));
      }
    }).then((unlisten) => {
      if (alive) stop = unlisten;
      else unlisten();
    });
    loadMedia(generation, undefined, false);
    return () => {
      alive = false;
      stop?.();
      dropMedia();
    };
  }, []);

  useEffect(() => {
    if (phase !== "saved" || format !== "mp4") return;
    const video = videoRef.current;
    if (!video) return;
    video.muted = false;
    void video.play().catch(() => {
      video.muted = true;
      void video.play().catch(() => undefined);
    });
  }, [phase, format, mediaUrl]);

  useEffect(() => {
    if (phase !== "recording") return;
    const started = Date.now();
    const timer = window.setInterval(() => {
      setElapsed(Date.now() - started);
    }, 200);
    return () => window.clearInterval(timer);
  }, [phase]);

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if (zoom.onKeyDown(event)) return;
    if (event.key === "Escape" && phase === "recording") {
      event.preventDefault();
      void screenshotRecordStop();
      return;
    }
    if (event.key === "Escape" && phase === "saved") {
      event.preventDefault();
      void screenshotRecordClose();
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

  async function downloadFile() {
    try {
      const saved = await screenshotRecordDownload();
      setNotice(t("zdx.text.screenshot.saved", { path: saved.path }));
    } catch (error) {
      setNotice(screenshotErrorText(t, toScreenshotError(error)));
    }
  }

  async function copyFile() {
    try {
      await screenshotRecordCopy();
      await screenshotRecordClose();
    } catch (error) {
      setNotice(screenshotErrorText(t, toScreenshotError(error)));
    }
  }

  if (phase === "saved") {
    return (
      <div
        ref={rootRef}
        tabIndex={0}
        role="dialog"
        aria-label={t("zdx.label.screenshot.recordDrag")}
        className={dragging ? "relative h-screen w-screen cursor-grabbing overflow-hidden bg-black text-white outline-none" : "relative h-screen w-screen cursor-grab overflow-hidden bg-black text-white outline-none"}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
      >
        {mediaUrl && format === "mp4" ? (
          <PreviewViewport zoom={zoom}>
            <video ref={videoRef} src={mediaUrl} aria-label={t("zdx.label.screenshot.recordPreview")} className="pointer-events-none h-full w-full object-contain" autoPlay loop playsInline />
          </PreviewViewport>
        ) : mediaUrl ? (
          <PreviewViewport zoom={zoom}>
            <img src={mediaUrl} alt={t("zdx.label.screenshot.recordPreview")} className="pointer-events-none h-full w-full object-contain" draggable={false} />
          </PreviewViewport>
        ) : (
          <div className="h-full w-full" aria-busy="true" />
        )}
        <div className="absolute top-2 right-2 flex cursor-default gap-1 rounded-lg p-1" style={{ background: "rgba(28, 28, 28, 0.92)" }} onPointerDown={(event) => event.stopPropagation()}>
          <ZoomButtons zoom={zoom} />
          <PreviewButton label={t("zdx.label.screenshot.download")} onClick={() => void downloadFile()}>
            <PreviewGlyph name="download" />
          </PreviewButton>
          <PreviewButton label={t("zdx.label.screenshot.copy")} onClick={() => void copyFile()}>
            <PreviewGlyph name="copy" />
          </PreviewButton>
          <PreviewButton label={t("zdx.label.screenshot.recordClose")} onClick={() => void screenshotRecordClose()}>
            <PreviewGlyph name="close" />
          </PreviewButton>
        </div>
        {notice && <div className="absolute bottom-2 left-2 right-2 rounded bg-black/80 px-2 py-1 text-xs text-white">{notice}</div>}
      </div>
    );
  }

  const status = phase === "exporting"
    ? t("zdx.text.screenshot.recordExporting", { format: formatLabel(format) })
    : phase === "error"
      ? message
      : t("zdx.text.screenshot.recordClock", { time: formatClock(elapsed) });

  return (
    <div className="flex h-full w-full p-2">
      <div
        ref={rootRef}
        tabIndex={0}
        role="dialog"
        aria-label={t("zdx.label.screenshot.modeRecord")}
        className="flex min-w-0 flex-1 items-center gap-2 rounded-[20px] px-3 text-white outline-none"
        style={{ background: "rgba(28, 28, 28, 0.92)" }}
        onKeyDown={onKeyDown}
      >
        {phase === "recording" ? <span className="h-2 w-2 shrink-0 rounded-full bg-[#E5484D]" /> : null}
        <span className="min-w-0 flex-1 truncate text-sm tabular-nums">{status}</span>
        {phase === "recording" ? (
          <button
            type="button"
            aria-label={t("zdx.label.screenshot.recordStop")}
            onPointerDown={(event) => {
              event.preventDefault();
              event.stopPropagation();
              void screenshotRecordStop();
            }}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border-0 bg-[#E5484D] p-0 text-white hover:bg-[#F2555A]"
          >
            <span className="block h-2.5 w-2.5 rounded-sm bg-white" />
          </button>
        ) : null}
      </div>
    </div>
  );
}

function PreviewButton({
  label,
  placement = "bottom",
  onClick,
  children,
}: {
  label: string;
  placement?: "top" | "bottom";
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip title={label} placement={placement} mouseEnterDelay={0.2}>
      <span className="inline-flex">
        <button
          type="button"
          aria-label={label}
          onClick={onClick}
          className="flex h-8 w-8 items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white hover:bg-white/15"
        >
          {children}
        </button>
      </span>
    </Tooltip>
  );
}

function PreviewGlyph({ name }: { name: "copy" | "close" | "download" }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      {name === "download" && <path d="M12 4 v10 M8 10 l4 4 4-4 M5 19 h14" />}
      {name === "copy" && <path d="M8 8 h10 v12 H8 z M6 16 H5 V4 h10 v2" />}
      {name === "close" && <path d="M7 7 l10 10 M17 7 L7 17" />}
    </svg>
  );
}
