import { Channel, invoke } from "@tauri-apps/api/core";

import type { SshHost, SshHostDraft, SshSessionEvent, SshStatus } from "@/types/ssh";
import type { SshTransferEvent } from "@/utils/sshTransfer";
import { isDesktop } from "@/utils/desktop";

export class SshRequestError extends Error {
  readonly code: string;
  readonly params: Record<string, unknown>;

  constructor(code: string, params: Record<string, unknown> = {}) {
    super(code);
    this.name = "SshRequestError";
    this.code = code;
    this.params = params;
  }
}

function assertDesktop() {
  if (!isDesktop()) throw new SshRequestError("desktop_only");
}

export function toSshError(error: unknown): SshRequestError {
  if (error instanceof SshRequestError) return error;
  if (typeof error === "string") {
    try {
      return toSshError(JSON.parse(error));
    } catch {
      return new SshRequestError("unknown");
    }
  }
  if (error && typeof error === "object" && "code" in error) {
    const record = error as { code?: unknown; params?: unknown };
    const params = record.params && typeof record.params === "object" ? record.params as Record<string, unknown> : {};
    return new SshRequestError(String(record.code), params);
  }
  return new SshRequestError("unknown");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  assertDesktop();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toSshError(error);
  }
}

export const sshStatus = () => call<SshStatus>("ssh_status");
export const sshSetEnabled = (enabled: boolean) => call<SshStatus>("ssh_set_enabled", { enabled });
export const sshHosts = () => call<SshHost[]>("ssh_hosts_list");
export const sshSaveHost = (host: SshHostDraft) => call<SshHost>("ssh_host_save", { host });
export const sshDeleteHost = (id: number) => call<void>("ssh_host_delete", { id });
export const sshForgetKey = (id: number) => call<void>("ssh_host_forget_key", { id });
export const sshPickKey = (title: string) => call<string | null>("ssh_pick_key", { title });

export function sshConnect(
  hostId: number,
  cols: number,
  rows: number,
  output: Channel<unknown>,
  events: Channel<SshSessionEvent>,
) {
  return call<number>("ssh_connect", { hostId, cols, rows, output, events });
}

export const sshHostKeyDecide = (sessionId: number, accept: boolean) =>
  call<void>("ssh_hostkey_decide", { sessionId, accept });

export const sshAnswer = (sessionId: number, promptId: number, secret: string, remember: boolean, cancel: boolean) =>
  call<void>("ssh_answer", { sessionId, promptId, secret, remember, cancel });

export function sshWrite(sessionId: number, data: Uint8Array) {
  return call<void>("ssh_write", { sessionId, data: Array.from(data) });
}

export const sshResize = (sessionId: number, cols: number, rows: number) =>
  call<void>("ssh_resize", { sessionId, cols, rows });

export const sshDisconnect = (sessionId: number) => call<void>("ssh_disconnect", { sessionId });

export type SshUploadPick = { token: string; names: string[] };

export const sshPickUpload = (kind: "files" | "folder", title: string) =>
  call<SshUploadPick | null>("ssh_pick_upload", { kind, title });

export const sshReleaseUpload = (token: string) => call<void>("ssh_release_upload", { token });

export function sshTransferStart(sessionId: number, token: string, progress: Channel<SshTransferEvent>) {
  return call<number>("ssh_transfer_start", { sessionId, token, progress });
}

export const sshTransferDecide = (taskId: number, policy: "overwrite" | "rename" | "skip", applyAll: boolean) =>
  call<void>("ssh_transfer_decide", { taskId, policy, applyAll });

export const sshTransferCancel = (taskId: number) => call<void>("ssh_transfer_cancel", { taskId });
