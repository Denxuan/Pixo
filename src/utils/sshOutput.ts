export function createOutputAck(options: {
  delayMs?: number;
  sessionId: () => number;
  send: (sessionId: number, bytes: number) => Promise<void> | void;
}) {
  const delayMs = options.delayMs ?? 16;
  let pending = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;

  const schedule = () => {
    if (timer !== undefined || closed || pending <= 0) return;
    timer = setTimeout(() => {
      timer = undefined;
      if (closed) return;
      const id = options.sessionId();
      if (id === 0 || pending <= 0) {
        if (pending > 0) schedule();
        return;
      }
      const bytes = pending;
      pending = 0;
      Promise.resolve(options.send(id, bytes)).catch(() => {
        if (closed) return;
        pending += bytes;
        schedule();
      });
    }, delayMs);
  };

  return {
    note(bytes: number) {
      if (closed || !Number.isFinite(bytes) || bytes <= 0) return;
      pending += bytes;
      schedule();
    },
    close() {
      closed = true;
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      pending = 0;
    },
  };
}
