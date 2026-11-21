import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { sshReleaseUpload } from "@/api/ssh";
import { isDesktop } from "@/utils/desktop";
import { dropHitsAny, type DropRect } from "@/utils/sshTransfer";

export type SshDropTarget = {
  rect: () => DropRect | null;
  ready: () => boolean;
  hover: (active: boolean) => void;
  upload: (token: string) => void;
  reject: () => void;
};

const targets = new Set<SshDropTarget>();
let dragUnlisten: UnlistenFn | null = null;
let dropUnlisten: UnlistenFn | null = null;
let starting: Promise<void> | null = null;

export function registerSshDropTarget(target: SshDropTarget) {
  targets.add(target);
  void ensureDropListeners();
  return () => {
    targets.delete(target);
    target.hover(false);
    if (targets.size === 0) {
      dragUnlisten?.();
      dropUnlisten?.();
      dragUnlisten = null;
      dropUnlisten = null;
    }
  };
}

function pointHits(x: number, y: number, rect: DropRect | null) {
  if (!rect) return false;
  const windows = navigator.userAgent.includes("Windows");
  return dropHitsAny(rect, x, y, windows, window.devicePixelRatio || 1);
}

async function ensureDropListeners() {
  if (!isDesktop() || dragUnlisten || dropUnlisten || starting) return;
  starting = (async () => {
    const drag = await listen<{ phase?: string; x?: number; y?: number }>("ssh://drag", (event) => {
      const phase = event.payload.phase;
      const x = Number(event.payload.x);
      const y = Number(event.payload.y);
      for (const target of targets) {
        target.hover(phase === "over" && pointHits(x, y, target.rect()));
      }
    });
    const drop = await listen<{ token?: string; x?: number; y?: number }>("ssh://dropped", (event) => {
      const token = event.payload.token;
      if (!token) return;
      const x = Number(event.payload.x);
      const y = Number(event.payload.y);
      for (const target of targets) target.hover(false);
      const hit = [...targets].find((target) => pointHits(x, y, target.rect()));
      if (!hit) {
        void sshReleaseUpload(token);
        return;
      }
      if (!hit.ready()) {
        void sshReleaseUpload(token);
        hit.reject();
        return;
      }
      hit.upload(token);
    });
    if (targets.size === 0) {
      drag();
      drop();
      return;
    }
    dragUnlisten = drag;
    dropUnlisten = drop;
  })().finally(() => {
    starting = null;
  });
  await starting;
}
