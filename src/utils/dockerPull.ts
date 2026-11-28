export type PullLayer = {
  id: string;
  status: string;
  current: number;
  total: number;
};

export type PullProgress = {
  headline: string;
  layers: PullLayer[];
};

const FINISHED = new Set(["Download complete", "Pull complete", "Already exists"]);

export function emptyPull(): PullProgress {
  return { headline: "", layers: [] };
}

export function applyPullProgress(
  state: PullProgress,
  event: { status: string; id: string; current: number; total: number },
): PullProgress {
  if (!event.id) {
    return { ...state, headline: event.status || state.headline };
  }
  const layers = state.layers.slice();
  const index = layers.findIndex((layer) => layer.id === event.id);
  const previous = index >= 0 ? layers[index] : undefined;
  const total = event.total > 0 ? event.total : previous?.total ?? 0;
  let current = event.current > 0 ? event.current : previous?.current ?? 0;
  if (FINISHED.has(event.status) && total > 0) current = total;
  if (total > 0) current = Math.min(current, total);
  const next = { id: event.id, status: event.status, current, total };
  if (index >= 0) layers[index] = next;
  else layers.push(next);
  return { headline: state.headline, layers };
}

export function pullTotals(layers: PullLayer[]): { current: number; total: number; percent: number } {
  let current = 0;
  let total = 0;
  for (const layer of layers) {
    if (layer.total <= 0) continue;
    total += layer.total;
    current += Math.min(Math.max(layer.current, 0), layer.total);
  }
  const percent = total > 0 ? Math.min(100, Math.round((current / total) * 100)) : 0;
  return { current, total, percent };
}
