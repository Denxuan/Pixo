import { listen } from "@tauri-apps/api/event";

import {
  clipboardCopy,
  clipboardDelete,
  clipboardGetSettings,
  clipboardList,
  clipboardReadNow,
  clipboardSetPaused,
  clipboardSetPinned,
  clipboardStatus,
  clipboardUpdateSettings,
  toClipboardError,
  type ClipboardRequestError,
} from "@/api/clipboard";
import type { ClipFilter, ClipSummary, ClipboardSettings, ClipboardSettingsUpdate, ClipboardStatus, CopyResult } from "@/types/clipboard";
import { isDesktop } from "@/utils/desktop";

function filterQuery(filter: ClipFilter): { kind?: "text" | "image" | "files"; pinnedOnly: boolean } {
  if (filter === "pinned") return { pinnedOnly: true };
  if (filter === "all") return { pinnedOnly: false };
  return { kind: filter, pinnedOnly: false };
}

export function useClipboardHistory() {
  const [status, setStatus] = useState<ClipboardStatus | null>(null);
  const [settings, setSettings] = useState<ClipboardSettings | null>(null);
  const [items, setItems] = useState<ClipSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [filter, setFilter] = useState<ClipFilter>("all");
  const [loading, setLoading] = useState(isDesktop());
  const [error, setError] = useState<ClipboardRequestError | null>(null);
  const request = useRef(0);
  const loadRef = useRef<(cursor?: string) => Promise<void>>(async () => {});

  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedQuery(query), 200);
    return () => window.clearTimeout(timer);
  }, [query]);

  const load = useCallback(async (cursor?: string) => {
    if (!isDesktop()) {
      setLoading(false);
      return;
    }
    const token = ++request.current;
    setLoading(true);
    try {
      const selected = filterQuery(filter);
      const page = await clipboardList({
        query: debouncedQuery,
        kind: selected.kind,
        pinnedOnly: selected.pinnedOnly,
        cursor,
        limit: 50,
      });
      if (token !== request.current) return;
      setItems((current) => cursor ? [...current, ...page.items] : page.items);
      setNextCursor(page.nextCursor);
      if (!cursor) setStatus(await clipboardStatus());
      setError(null);
    } catch (caught) {
      if (token !== request.current) return;
      setError(toClipboardError(caught));
    } finally {
      if (token === request.current) setLoading(false);
    }
  }, [debouncedQuery, filter]);

  loadRef.current = load;

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!isDesktop()) return;
    let alive = true;
    const stops: Array<() => void> = [];
    void listen("clipboard://changed", () => {
      void loadRef.current();
    }).then((stop) => {
      if (alive) stops.push(stop);
      else stop();
    });
    void listen<ClipboardStatus>("clipboard://status", (event) => {
      setStatus(event.payload);
    }).then((stop) => {
      if (alive) stops.push(stop);
      else stop();
    });
    return () => {
      alive = false;
      stops.forEach((stop) => stop());
    };
  }, []);

  async function run<T>(action: () => Promise<T>): Promise<T> {
    try {
      const value = await action();
      setError(null);
      return value;
    } catch (caught) {
      const next = toClipboardError(caught);
      if (next.code !== "needs_confirm") setError(next);
      throw next;
    }
  }

  return {
    status,
    settings,
    setSettings,
    items,
    nextCursor,
    query,
    setQuery,
    filter,
    setFilter,
    loading,
    error,
    setError,
    reload: () => load(),
    loadMore: () => nextCursor ? load(nextCursor) : Promise.resolve(),
    refreshStatus: () => run(async () => {
      const next = await clipboardStatus();
      setStatus(next);
      return next;
    }),
    copyItem: (id: number) => run(() => clipboardCopy(id)) as Promise<CopyResult>,
    pinItem: (id: number, pinned: boolean) => run(async () => {
      await clipboardSetPinned(id, pinned);
      setItems((current) => current.map((item) => item.id === id ? { ...item, pinned } : item));
    }),
    deleteItem: (id: number) => run(async () => {
      await clipboardDelete([id]);
      setItems((current) => current.filter((item) => item.id !== id));
    }),
    readNow: () => run(() => clipboardReadNow()),
    setPaused: (paused: boolean) => run(async () => {
      setStatus(await clipboardSetPaused(paused));
    }),
    loadSettings: () => run(async () => {
      const current = await clipboardGetSettings();
      setSettings(current);
      return current;
    }),
    saveSettings: (update: ClipboardSettingsUpdate) => run(async () => {
      const saved = await clipboardUpdateSettings(update);
      setSettings(saved);
      setStatus(await clipboardStatus());
      return saved;
    }),
  };
}
