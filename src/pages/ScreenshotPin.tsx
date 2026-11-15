import { listen } from "@tauri-apps/api/event";
import { LogicalPosition, getCurrentWindow } from "@tauri-apps/api/window";

import { screenshotPinClose, screenshotPinCopy, screenshotPinImage, screenshotPinReady, toScreenshotError } from "@/api/screenshot";
import { PreviewViewport, ZoomButtons, usePreviewZoom } from "@/components/PreviewZoom";
import { draggedOrigin, pinSlot } from "@/utils/screenshotGeometry";
import { screenshotErrorText } from "@/utils/screenshotMessage";

function readSlot() {
  try {
    return pinSlot(getCurrentWindow().label);
  } catch {
    return null;
  }
}

export default function ScreenshotPin() {
  const { t } = useTranslation();
  const [slot] = useState(readSlot);
  const [image, setImage] = useState<{ url: string; generation: number } | null>(null);
  const [notice, setNotice] = useState("");
  const [dragging, setDragging] = useState(false);
  const readyRef = useRef(false);
  const generationRef = useRef(0);
  const zoom = usePreviewZoom();

  function tellReady() {
    if (readyRef.current || slot == null) return;
    readyRef.current = true;
    void screenshotPinReady(slot).catch(() => undefined);
  }

  useEffect(() => {
    if (slot == null) return;
    let alive = true;
    let objectUrl = "";
    const show = () => {
      const generation = generationRef.current + 1;
      generationRef.current = generation;
      readyRef.current = false;
      void screenshotPinImage(slot).then((bytes) => {
        if (!alive || generationRef.current !== generation) return;
        if (objectUrl) URL.revokeObjectURL(objectUrl);
        const copy = new Uint8Array(bytes);
        objectUrl = URL.createObjectURL(new Blob([copy], { type: "image/png" }));
        setNotice("");
        setImage({ url: objectUrl, generation });
        zoom.reset();
      }).catch((error) => {
        if (!alive || generationRef.current !== generation) return;
        if (toScreenshotError(error).code === "not_found") return;
        setNotice(screenshotErrorText(t, toScreenshotError(error)));
        tellReady();
      });
    };
    const clear = () => {
      generationRef.current += 1;
      readyRef.current = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      objectUrl = "";
      setImage(null);
    };
    const subscription = Promise.all([
      listen<{ slot: number }>("screenshot://pin-show", (event) => {
        if (event.payload.slot === slot) show();
      }),
      listen<{ slot: number }>("screenshot://pin-clear", (event) => {
        if (event.payload.slot === slot) clear();
      }),
    ]).then((stops) => {
      if (!alive) {
        stops.forEach((stop) => stop());
        return;
      }
      show();
      return stops;
    });
    return () => {
      alive = false;
      generationRef.current += 1;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      void subscription.then((stops) => stops?.forEach((stop) => stop()));
    };
  }, [slot, t]);

  function onImageLoad(generation: number) {
    const finish = () => {
      if (generationRef.current !== generation) return;
      tellReady();
    };
    requestAnimationFrame(() => {
      requestAnimationFrame(finish);
    });
    window.setTimeout(finish, 150);
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

  return (
    <div
      tabIndex={0}
      aria-label={t("zdx.label.screenshot.dragPin")}
      className={dragging ? "relative h-screen w-screen cursor-grabbing overflow-hidden bg-black outline-none" : "relative h-screen w-screen cursor-grab overflow-hidden bg-black outline-none"}
      onPointerDown={onPointerDown}
      onKeyDown={(event) => {
        zoom.onKeyDown(event);
      }}
    >
      <PreviewViewport zoom={zoom}>
        {image && <img src={image.url} alt={t("zdx.label.screenshot.pin")} className="pointer-events-none h-full w-full" draggable={false} decoding="sync" onLoad={() => onImageLoad(image.generation)} />}
      </PreviewViewport>
      <div className="absolute top-2 right-2 flex cursor-default gap-1 rounded-lg p-1" style={{ background: "rgba(28, 28, 28, 0.92)" }}>
        <ZoomButtons zoom={zoom} />
        <PinButton label={t("zdx.label.screenshot.pinCopy")} disabled={slot == null} onClick={() => { if (slot != null) void screenshotPinCopy(slot).catch((error) => setNotice(screenshotErrorText(t, toScreenshotError(error)))); }}>
          <PinGlyph name="copy" />
        </PinButton>
        <PinButton label={t("zdx.label.screenshot.pinClose")} disabled={slot == null} onClick={() => { if (slot != null) void screenshotPinClose(slot).catch(() => undefined); }}>
          <PinGlyph name="close" />
        </PinButton>
      </div>
      {notice && <div className="absolute bottom-2 left-2 right-2 rounded bg-black/80 px-2 py-1 text-xs text-white">{notice}</div>}
      {!image && !notice && <div className="h-full w-full" aria-busy="true" />}
    </div>
  );
}

function PinButton({
  label,
  disabled = false,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip title={label} placement="bottom" mouseEnterDelay={0.2}>
      <span className="inline-flex">
        <button
          type="button"
          aria-label={label}
          disabled={disabled}
          onClick={onClick}
          className={disabled
            ? "flex h-8 w-8 cursor-not-allowed items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white opacity-40"
            : "flex h-8 w-8 items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white hover:bg-white/15"}
        >
          {children}
        </button>
      </span>
    </Tooltip>
  );
}

function PinGlyph({ name }: { name: "copy" | "close" }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      {name === "copy" && <path d="M8 8 h10 v12 H8 z M6 16 H5 V4 h10 v2" />}
      {name === "close" && <path d="M7 7 l10 10 M17 7 L7 17" />}
    </svg>
  );
}
