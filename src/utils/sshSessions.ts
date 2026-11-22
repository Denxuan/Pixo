const MAX_OPEN = 12;

export function parseOpenHosts(raw: string | null): number[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((item): item is number => typeof item === "number" && Number.isInteger(item) && item > 0)
      .slice(0, MAX_OPEN);
  } catch {
    return [];
  }
}

export function readOpenHosts(): number[] {
  try {
    return parseOpenHosts(localStorage.getItem("pixo-ssh-sessions"));
  } catch {
    return [];
  }
}

export function writeOpenHosts(ids: number[]) {
  try {
    localStorage.setItem("pixo-ssh-sessions", JSON.stringify(ids.slice(0, MAX_OPEN)));
  } catch {
    // The list is only a convenience. A full store should not block the page.
  }
}

const RECENT_KEY = "pixo-ssh-recent";
const MAX_RECENT = 8;

export function pushRecent(ids: number[], id: number, limit = MAX_RECENT): number[] {
  if (!Number.isInteger(id) || id <= 0) return ids.slice(0, limit);
  return [id, ...ids.filter((item) => item !== id)].slice(0, limit);
}

export function readRecentHosts(): number[] {
  try {
    return parseOpenHosts(localStorage.getItem(RECENT_KEY)).slice(0, MAX_RECENT);
  } catch {
    return [];
  }
}

export function rememberHost(id: number) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(pushRecent(readRecentHosts(), id)));
  } catch {
    // Recent hosts only change what the quick panel shows first.
  }
}
