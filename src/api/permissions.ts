import { invoke } from "@tauri-apps/api/core";

import { isDesktop } from "@/utils/desktop";

export interface SystemPermission {
  id: "accessibility" | "inputMonitoring" | "pasteboard";
  granted: boolean;
}

export async function permissionsStatus(): Promise<SystemPermission[]> {
  if (!isDesktop()) return [];
  return invoke<SystemPermission[]>("permissions_status");
}

export async function permissionsPrepare(): Promise<SystemPermission[]> {
  if (!isDesktop()) return [];
  return invoke<SystemPermission[]>("permissions_prepare");
}

export async function permissionsOpen(id: SystemPermission["id"]): Promise<void> {
  if (!isDesktop()) return;
  await invoke("permissions_open", { id });
}
