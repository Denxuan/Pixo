import { clampHostPane, DEFAULT_HOST_PANE, hostPaneBounds, readHostPaneWidth } from "@/utils/sshLayout";

const HOST_PANE_KEY = "pixo-ssh-host-pane";

function storedHostPane(): number {
  try {
    return readHostPaneWidth(localStorage.getItem(HOST_PANE_KEY));
  } catch {
    return DEFAULT_HOST_PANE;
  }
}

function rememberHostPane(width: number) {
  try {
    localStorage.setItem(HOST_PANE_KEY, String(width));
  } catch {
    // The width still applies for this visit when storage is blocked.
  }
}

export default function HostSplit({ start, end }: { start: ReactNode; end: ReactNode }) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const paneRef = useRef<HTMLDivElement>(null);
  const asideRef = useRef<HTMLElement>(null);
  const [paneWidth, setPaneWidth] = useState(storedHostPane);
  const [containerWidth, setContainerWidth] = useState(0);
  const [hot, setHot] = useState(false);
  const [resizing, setResizing] = useState(false);
  const savedWidth = useRef(paneWidth);
  const drag = useRef<{ pointerId: number; x: number; width: number } | null>(null);
  const bounds = hostPaneBounds(containerWidth);

  useEffect(() => {
    const node = paneRef.current;
    if (!node) return;
    const apply = () => {
      const container = node.getBoundingClientRect().width;
      if (container < 1) return;
      setContainerWidth(container);
      if (drag.current) return;
      setPaneWidth(clampHostPane(savedWidth.current, container));
    };
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!resizing) return;
    const previousCursor = document.body.style.cursor;
    const previousSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    return () => {
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousSelect;
    };
  }, [resizing]);

  function commit(width: number) {
    const container = paneRef.current?.getBoundingClientRect().width ?? 0;
    const next = clampHostPane(width, container);
    savedWidth.current = next;
    setPaneWidth(next);
    rememberHostPane(next);
  }

  function onPointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    event.preventDefault();
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // The drag still tracks move events when the browser cannot capture this pointer.
    }
    drag.current = {
      pointerId: event.pointerId,
      x: event.clientX,
      width: asideRef.current?.getBoundingClientRect().width ?? paneWidth,
    };
    setResizing(true);
  }

  function onPointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const start = drag.current;
    const container = paneRef.current?.getBoundingClientRect().width ?? 0;
    if (!start || start.pointerId !== event.pointerId || container < 1) return;
    setPaneWidth(clampHostPane(start.width + event.clientX - start.x, container));
  }

  function endDrag(event: React.PointerEvent<HTMLDivElement>) {
    const start = drag.current;
    if (!start || start.pointerId !== event.pointerId) return;
    drag.current = null;
    setResizing(false);
    setHot(event.currentTarget.matches(":hover"));
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (Math.abs(event.clientX - start.x) < 2) return;
    commit(start.width + event.clientX - start.x);
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const current = asideRef.current?.getBoundingClientRect().width ?? paneWidth;
    const step = event.shiftKey ? 48 : 16;
    if (event.key === "ArrowLeft") commit(current - step);
    else if (event.key === "ArrowRight") commit(current + step);
    else if (event.key === "Home") commit(bounds.min);
    else if (event.key === "End") commit(bounds.max);
    else return;
    event.preventDefault();
  }

  return (
    <div ref={paneRef} className="flex h-full min-h-0 min-w-0 flex-1">
      <aside ref={asideRef} className="flex h-full min-h-0 shrink-0 flex-col overflow-hidden" style={{ width: paneWidth }}>
        {start}
      </aside>
      <div className="relative z-20 w-px shrink-0" style={{ background: hot || resizing ? token.colorPrimary : token.colorBorderSecondary }}>
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={t("zdx.label.ssh.resizeHosts")}
          aria-valuemin={bounds.min}
          aria-valuemax={bounds.max}
          aria-valuenow={paneWidth}
          tabIndex={0}
          className="absolute -left-1.5 top-0 h-full w-3 cursor-col-resize touch-none select-none outline-none focus-visible:ring-2 focus-visible:ring-[#4f5df0]"
          onPointerEnter={() => setHot(true)}
          onPointerLeave={() => { if (!drag.current) setHot(false); }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onDoubleClick={() => commit(DEFAULT_HOST_PANE)}
          onKeyDown={onKeyDown}
        />
      </div>
      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        {end}
      </section>
    </div>
  );
}
