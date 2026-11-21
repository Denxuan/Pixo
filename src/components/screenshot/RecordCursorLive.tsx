type Selection = { x: number; y: number; w: number; h: number };
type Ripple = { id: number; x: number; y: number };

export function RecordCursorLive({
  surfaceRef,
  selection,
  highlight,
  click,
}: {
  surfaceRef: React.RefObject<HTMLElement | null>;
  selection: Selection;
  highlight: boolean;
  click: boolean;
}) {
  const ringRef = useRef<HTMLSpanElement>(null);
  const [ripples, setRipples] = useState<Ripple[]>([]);
  const nextId = useRef(0);
  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  const removeRipple = useCallback((id: number) => {
    setRipples((current) => current.filter((item) => item.id !== id));
  }, []);

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;
    const local = (event: PointerEvent) => {
      const rect = surface.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return null;
      return {
        x: (event.clientX - rect.left) * (surface.clientWidth / rect.width),
        y: (event.clientY - rect.top) * (surface.clientHeight / rect.height),
      };
    };
    const inside = (point: { x: number; y: number }) => {
      const box = selectionRef.current;
      return point.x >= box.x && point.y >= box.y && point.x <= box.x + box.w && point.y <= box.y + box.h;
    };
    const placeRing = (point: { x: number; y: number } | null) => {
      const ring = ringRef.current;
      if (!ring) return;
      if (!point) {
        ring.style.opacity = "0";
        return;
      }
      const box = selectionRef.current;
      ring.style.opacity = "1";
      ring.style.left = `${point.x - box.x}px`;
      ring.style.top = `${point.y - box.y}px`;
    };
    const onMove = (event: PointerEvent) => {
      const point = local(event);
      placeRing(point && inside(point) ? point : null);
    };
    const onDown = (event: PointerEvent) => {
      if (!click || event.button !== 0) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("[data-shot-ui]")) return;
      const point = local(event);
      if (!point || !inside(point)) return;
      const box = selectionRef.current;
      const id = nextId.current + 1;
      nextId.current = id;
      setRipples((current) => [...current, { id, x: point.x - box.x, y: point.y - box.y }]);
    };
    const onLeave = () => placeRing(null);
    surface.addEventListener("pointermove", onMove);
    surface.addEventListener("pointerleave", onLeave);
    // The overlay stops pointerdown while it is still going down, so the click
    // has to be read on the window before that happens.
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      surface.removeEventListener("pointermove", onMove);
      surface.removeEventListener("pointerleave", onLeave);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [click, surfaceRef]);

  return (
    <div
      className="pointer-events-none absolute z-10 overflow-hidden"
      style={{ left: selection.x, top: selection.y, width: selection.w, height: selection.h }}
    >
      {highlight && (
        <span
          ref={ringRef}
          className="absolute h-11 w-11 rounded-full border-[3px] border-[#3B82F6] bg-[#3B82F6]/30 opacity-0"
          style={{ transform: "translate(-50%, -50%)" }}
        />
      )}
      {ripples.map((ripple) => (
        <ClickBurst key={ripple.id} ripple={ripple} onDone={removeRipple} />
      ))}
    </div>
  );
}

function ClickBurst({ ripple, onDone }: { ripple: Ripple; onDone: (id: number) => void }) {
  const ringRef = useRef<HTMLSpanElement>(null);
  const dotRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    const ring = ringRef.current?.animate(
      [
        { transform: "translate(-50%, -50%) scale(0.35)", opacity: 0.95 },
        { transform: "translate(-50%, -50%) scale(1.85)", opacity: 0 },
      ],
      { duration: 450, easing: "ease-out", fill: "forwards" },
    );
    const dot = dotRef.current?.animate(
      [
        { transform: "translate(-50%, -50%) scale(1)", opacity: 1 },
        { transform: "translate(-50%, -50%) scale(0.4)", opacity: 0 },
      ],
      { duration: 450, easing: "ease-out", fill: "forwards" },
    );
    const timer = window.setTimeout(() => onDone(ripple.id), 460);
    return () => {
      ring?.cancel();
      dot?.cancel();
      window.clearTimeout(timer);
    };
  }, [onDone, ripple.id]);

  return (
    <>
      <span
        ref={ringRef}
        className="absolute h-11 w-11 rounded-full border-[3px] border-white"
        style={{ left: ripple.x, top: ripple.y, transform: "translate(-50%, -50%)" }}
      />
      <span
        ref={dotRef}
        className="absolute h-4 w-4 rounded-full bg-white"
        style={{ left: ripple.x, top: ripple.y, transform: "translate(-50%, -50%)" }}
      />
    </>
  );
}
