import { expect, test } from "vitest";

import { applyPullProgress, emptyPull, pullTotals } from "@/utils/dockerPull";

test("keeps one row per layer and shows downloaded bytes", () => {
    let state = emptyPull();
    state = applyPullProgress(state, { id: "", status: "Pulling from library/postgres", current: 0, total: 0 });
    state = applyPullProgress(state, { id: "a1361b767ece", status: "Downloading", current: 4_000_000, total: 28_000_000 });
    state = applyPullProgress(state, { id: "a1361b767ece", status: "Downloading", current: 12_000_000, total: 28_000_000 });
    state = applyPullProgress(state, { id: "40748add9f4f", status: "Downloading", current: 1_000, total: 8_000_000 });
    expect(state.headline).toBe("Pulling from library/postgres");
    expect(state.layers).toHaveLength(2);
    expect(state.layers[0]).toMatchObject({ id: "a1361b767ece", current: 12_000_000, total: 28_000_000 });
    expect(pullTotals(state.layers)).toEqual({ current: 12_001_000, total: 36_000_000, percent: 33 });
  });

test("fills the layer when the download finishes", () => {
    const state = applyPullProgress(emptyPull(), { id: "abc", status: "Downloading", current: 10, total: 100 });
    const done = applyPullProgress(state, { id: "abc", status: "Download complete", current: 0, total: 0 });
    expect(done.layers[0]).toMatchObject({ current: 100, total: 100, status: "Download complete" });
  });