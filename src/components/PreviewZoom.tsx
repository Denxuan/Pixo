import { clampPan, clampZoom, zoomAt, type Pan } from "@/utils/previewZoom";

const STEP = 1.25;

export type PreviewZoom = {
  scale: number;
  canZoomIn: boolean;
  canZoomOut: boolean;
  zoomIn: () => void;
  zoomOut: () => void;
  reset: () => void;
  onKeyDown: (event: React.KeyboardEvent) => boolean;
  frame: ZoomFrame;
};

type ZoomFrame = {
  boxRef: React.RefObject<HTMLDivElement | null>;
  scaleRef: React.RefObject<number>;
  panRef: React.RefObject<Pan>;
  commit: (scale: number, pan: Pan, width?: number, height?: number) => void;
  scale: number;
  pan: Pan;
};

export function usePreviewZoom(): PreviewZoom {
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState<Pan>({ x: 0, y: 0 });
  const scaleRef = useRef(1);
  const panRef = useRef<Pan>({ x: 0, y: 0 });
  const boxRef = useRef<HTMLDivElement | null>(null);

  const commit = useCallback((nextScale: number, nextPan: Pan, width = 0, height = 0) => {
    const clamped = clampZoom(nextScale);
    const limited = clamped <= 1 ? { x: 0, y: 0 } : clampPan(nextPan, clamped, width, height);
    scaleRef.current = clamped;
    panRef.current = limited;
    setScale(clamped);
    setPan(limited);
  }, []);

  const boxSize = useCallback(() => {
    const rect = boxRef.current?.getBoundingClientRect();
    return { width: rect?.width ?? 0, height: rect?.height ?? 0 };
  }, []);

  const zoomBy = useCallback((factor: number) => {
    const { width, height } = boxSize();
    const next = zoomAt(scaleRef.current, panRef.current, scaleRef.current * factor, { x: 0, y: 0 });
    commit(next.scale, next.pan, width, height);
  }, [boxSize, commit]);

  const zoomIn = useCallback(() => zoomBy(STEP), [zoomBy]);
  const zoomOut = useCallback(() => zoomBy(1 / STEP), [zoomBy]);
  const reset = useCallback(() => commit(1, { x: 0, y: 0 }), [commit]);

  const onKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      zoomIn();
      return true;
    }
    if (event.key === "-" || event.key === "_") {
      event.preventDefault();
      zoomOut();
      return true;
    }
    if (event.key === "0") {
      event.preventDefault();
      reset();
      return true;
    }
    return false;
  }, [reset, zoomIn, zoomOut]);

  const frame = useMemo<ZoomFrame>(() => ({ boxRef, scaleRef, panRef, commit, scale, pan }), [commit, pan, scale]);

  return {
    scale,
    canZoomIn: scale < clampZoom(Number.POSITIVE_INFINITY) - 0.01,
    canZoomOut: scale > 1.01,
    zoomIn,
    zoomOut,
    reset,
    onKeyDown,
    frame,
  };
}

export function PreviewViewport({
  zoom,
  align = "center",
  children,
}: {
  zoom: PreviewZoom;
  align?: "center" | "start";
  children: React.ReactNode;
}) {
  const { frame } = zoom;
  const [panning, setPanning] = useState(false);

  function onWheel(event: React.WheelEvent<HTMLDivElement>) {
    event.preventDefault();
    event.stopPropagation();
    const rect = event.currentTarget.getBoundingClientRect();
    const cursor = {
      x: event.clientX - rect.left - rect.width / 2,
      y: event.clientY - rect.top - rect.height / 2,
    };
    const factor = Math.exp(-event.deltaY * 0.0015);
    const next = zoomAt(frame.scaleRef.current, frame.panRef.current, frame.scaleRef.current * factor, cursor);
    frame.commit(next.scale, next.pan, rect.width, rect.height);
  }

  function onPointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || frame.scaleRef.current <= 1.01) return;
    if ((event.target as HTMLElement).closest("button")) return;
    event.preventDefault();
    event.stopPropagation();
    const surface = event.currentTarget;
    surface.setPointerCapture(event.pointerId);
    const origin = { x: event.clientX, y: event.clientY };
    const start = { ...frame.panRef.current };
    const rect = surface.getBoundingClientRect();
    let stopped = false;
    const move = (ev: PointerEvent) => {
      if (stopped) return;
      const next = clampPan(
        { x: start.x + ev.clientX - origin.x, y: start.y + ev.clientY - origin.y },
        frame.scaleRef.current,
        rect.width,
        rect.height,
      );
      frame.commit(frame.scaleRef.current, next, rect.width, rect.height);
    };
    const end = () => {
      stopped = true;
      setPanning(false);
      surface.removeEventListener("pointermove", move);
      surface.removeEventListener("pointerup", end);
      surface.removeEventListener("pointercancel", end);
      if (surface.hasPointerCapture(event.pointerId)) surface.releasePointerCapture(event.pointerId);
    };
    surface.addEventListener("pointermove", move);
    surface.addEventListener("pointerup", end);
    surface.addEventListener("pointercancel", end);
    setPanning(true);
  }

  function onDoubleClick(event: React.MouseEvent<HTMLDivElement>) {
    if (frame.scaleRef.current <= 1.01) return;
    event.preventDefault();
    event.stopPropagation();
    frame.commit(1, { x: 0, y: 0 });
  }

  const shell = panning
    ? "absolute inset-0 cursor-grabbing overflow-hidden"
    : frame.scale > 1.01
      ? "absolute inset-0 cursor-grab overflow-hidden"
      : "absolute inset-0 overflow-hidden";
  const stage = align === "start"
    ? "flex h-full w-full items-start justify-center"
    : "flex h-full w-full items-center justify-center";

  return (
    <div
      ref={frame.boxRef}
      className={shell}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onDoubleClick={onDoubleClick}
    >
      <div
        className={stage}
        style={{ transform: `translate(${frame.pan.x}px, ${frame.pan.y}px) scale(${frame.scale})` }}
      >
        {children}
      </div>
    </div>
  );
}

export function ZoomButtons({ zoom }: { zoom: PreviewZoom }) {
  const { t } = useTranslation();
  return (
    <>
      <ZoomButton label={t("zdx.label.screenshot.zoomIn")} disabled={!zoom.canZoomIn} onClick={zoom.zoomIn}>
        <ZoomGlyph name="in" />
      </ZoomButton>
      <ZoomButton label={t("zdx.label.screenshot.zoomOut")} disabled={!zoom.canZoomOut} onClick={zoom.zoomOut}>
        <ZoomGlyph name="out" />
      </ZoomButton>
    </>
  );
}

function ZoomButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
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
            : "flex h-8 w-8 cursor-default items-center justify-center rounded-lg border-0 bg-transparent p-0 text-white hover:bg-white/15"}
        >
          {children}
        </button>
      </span>
    </Tooltip>
  );
}

function ZoomGlyph({ name }: { name: "in" | "out" }) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="M15.5 15.5 L20 20" />
      {name === "in" ? <path d="M10.5 7.5 v6 M7.5 10.5 h6" /> : <path d="M7.5 10.5 h6" />}
    </svg>
  );
}
