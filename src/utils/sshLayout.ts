export const DEFAULT_HOST_PANE = 288;
export const MIN_HOST_PANE = 200;
export const MIN_TERMINAL_PANE = 320;

const STORED_LIMIT = 1600;

export function hostPaneBounds(container: number): { min: number; max: number } {
  if (!Number.isFinite(container) || container <= 0) {
    return { min: MIN_HOST_PANE, max: STORED_LIMIT };
  }
  const max = Math.max(160, container - MIN_TERMINAL_PANE);
  const min = Math.min(MIN_HOST_PANE, max);
  return { min, max: Math.max(min, max) };
}

export function clampHostPane(width: number, container: number): number {
  const { min, max } = hostPaneBounds(container);
  const value = Number.isFinite(width) ? width : DEFAULT_HOST_PANE;
  return Math.round(Math.min(max, Math.max(min, value)));
}

export function readHostPaneWidth(stored: string | null): number {
  if (stored == null || stored.trim() === "") return DEFAULT_HOST_PANE;
  const value = Number(stored);
  if (!Number.isFinite(value)) return DEFAULT_HOST_PANE;
  return clampHostPane(value, 0);
}
