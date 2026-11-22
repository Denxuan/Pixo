export function autosaveAction(input: {
  ready: boolean;
  inflight: boolean;
  saved: string | null;
  next: string | null;
}): "baseline" | "wait" | "save" {
  if (input.next == null) return "wait";
  if (!input.ready) return "baseline";
  if (input.next === input.saved && !input.inflight) return "wait";
  return "save";
}

export function useAutosave<T>(value: T | null, save: (value: T, isCurrent: () => boolean) => Promise<boolean>, delay = 250) {
  const saved = useRef<string | null>(null);
  const ready = useRef(false);
  const inflight = useRef(false);
  const savingKey = useRef<string | null>(null);
  const queued = useRef<T | null>(null);
  const epoch = useRef(0);
  const latest = useRef(value);
  const saveRef = useRef(save);
  const key = value == null ? null : JSON.stringify(value);

  useEffect(() => {
    latest.current = value;
    saveRef.current = save;
  });

  function kick() {
    if (inflight.current) return;
    const snapshot = queued.current;
    if (snapshot == null) return;
    const snapshotKey = JSON.stringify(snapshot);
    queued.current = null;
    if (snapshotKey === saved.current) return;
    inflight.current = true;
    savingKey.current = snapshotKey;
    const token = ++epoch.current;
    const isCurrent = () => token === epoch.current && queued.current == null;
    void (async () => {
      let ok = false;
      try {
        ok = await saveRef.current(snapshot, isCurrent);
      } catch {
        ok = false;
      }
      inflight.current = false;
      savingKey.current = null;
      if (token === epoch.current && ok && queued.current == null) saved.current = snapshotKey;
      if (queued.current != null) kick();
    })();
  }

  useEffect(() => {
    if (key == null) return;
    const action = autosaveAction({
      ready: ready.current,
      inflight: inflight.current,
      saved: saved.current,
      next: key,
    });
    if (action === "baseline") {
      ready.current = true;
      saved.current = key;
      return;
    }
    if (action === "wait") return;
    const snapshot = value as T;
    const timer = window.setTimeout(() => {
      queued.current = snapshot;
      kick();
    }, delay);
    return () => window.clearTimeout(timer);
  }, [delay, key, value]);

  useEffect(() => {
    return () => {
      const current = latest.current;
      if (current == null) return;
      const currentKey = JSON.stringify(current);
      if (!ready.current || currentKey === saved.current || currentKey === savingKey.current) return;
      queued.current = current;
      kick();
    };
  }, []);
}
