export function sshEndpoint(host: { username: string; host: string; port: number }) {
  const name = host.host.includes(":") && !host.host.startsWith("[") ? `[${host.host}]` : host.host;
  const target = `${host.username}@${name}`;
  return host.port === 22 ? target : `${target}:${host.port}`;
}

export function tabsAfterClose<T extends { key: string }>(
  tabs: T[],
  activeKey: string,
  shouldClose: (tab: T, index: number) => boolean,
  preferKey?: string,
): { tabs: T[]; activeKey: string } {
  const activeIndex = Math.max(0, tabs.findIndex((tab) => tab.key === activeKey));
  const next = tabs.filter((tab, index) => !shouldClose(tab, index));
  if (preferKey && next.some((tab) => tab.key === preferKey)) return { tabs: next, activeKey: preferKey };
  if (next.some((tab) => tab.key === activeKey)) return { tabs: next, activeKey };
  return { tabs: next, activeKey: next[Math.min(activeIndex, next.length - 1)]?.key ?? "" };
}

export function focusExistingSession(
  tabs: { key: string; hostId: number; phase: string }[],
  hostId: number,
): { key: string; reconnect: boolean } | null {
  const existing = tabs.find((tab) => tab.hostId === hostId);
  if (!existing) return null;
  return { key: existing.key, reconnect: existing.phase === "closed" || existing.phase === "idle" };
}
