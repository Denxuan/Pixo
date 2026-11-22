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
