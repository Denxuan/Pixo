import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  ContainerRow,
  DockerConnectEvent,
  DockerContextPreview,
  DockerEndpoint,
  DockerEndpointDraft,
  DockerLogEvent,
  DockerPullEvent,
  DockerSocket,
  DockerStatus,
  EngineSummary,
  HostDirListing,
  ImageRow,
  NetworkRow,
  ComposePreview,
  ComposeProjectRequest,
  ComposeProject,
  ContainerRunRequest,
} from "@/types/docker";
import { isDesktop } from "@/utils/desktop";

export class DockerRequestError extends Error {
  readonly code: string;
  readonly params: Record<string, unknown>;

  constructor(code: string, params: Record<string, unknown> = {}) {
    super(code);
    this.name = "DockerRequestError";
    this.code = code;
    this.params = params;
  }
}

function assertDesktop() {
  if (!isDesktop()) throw new DockerRequestError("desktop_only");
}

export function toDockerError(error: unknown): DockerRequestError {
  if (error instanceof DockerRequestError) return error;
  if (typeof error === "string") {
    try {
      return toDockerError(JSON.parse(error));
    } catch {
      return new DockerRequestError("unknown");
    }
  }
  if (error && typeof error === "object" && "code" in error) {
    const record = error as { code?: unknown; params?: unknown };
    const params = record.params && typeof record.params === "object" ? record.params as Record<string, unknown> : {};
    return new DockerRequestError(String(record.code), params);
  }
  return new DockerRequestError("unknown");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  assertDesktop();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toDockerError(error);
  }
}

export const dockerStatus = () => call<DockerStatus>("docker_status");
export const dockerSetEnabled = (enabled: boolean) => call<DockerStatus>("docker_set_enabled", { enabled });
export const dockerSetImageUpdates = (enabled: boolean) => call<DockerStatus>("docker_set_image_updates", { enabled });
export const dockerEndpoints = () => call<DockerEndpoint[]>("docker_endpoints_list");
export const dockerSaveEndpoint = (endpoint: DockerEndpointDraft) => call<DockerEndpoint>("docker_endpoint_save", { endpoint });
export const dockerDeleteEndpoint = (id: number) => call<void>("docker_endpoint_delete", { id });
export const dockerSockets = () => call<DockerSocket[]>("docker_local_sockets");
export const dockerContextPreview = () => call<DockerContextPreview[]>("docker_contexts_preview");
export const dockerContextImport = (indexes: number[]) => call<DockerEndpoint[]>("docker_contexts_import", { indexes });
export const dockerPickFile = (title: string) => call<string | null>("docker_pick_file", { title });
export const dockerDisconnect = (endpointId: number) => call<void>("docker_disconnect", { endpointId });
export const dockerRenew = (endpointId: number) => call<void>("docker_renew", { endpointId });
export const dockerHostKeyDecide = (linkId: number, accept: boolean) => call<void>("docker_hostkey_decide", { linkId, accept });
export const dockerAnswer = (linkId: number, promptId: number, secret: string, remember: boolean, cancel: boolean) =>
  call<void>("docker_answer", { linkId, promptId, secret, remember, cancel });
export const dockerSummary = (endpointId: number) => call<EngineSummary>("docker_summary", { endpointId });
export const dockerMirrors = (endpointId: number) => call<string[]>("docker_mirrors", { endpointId });
export const dockerSetMirrors = (endpointId: number, mirrors: string[]) => call<void>("docker_set_mirrors", { endpointId, mirrors });
export const dockerContainers = (endpointId: number) => call<ContainerRow[]>("docker_containers", { endpointId });
export const dockerContainerAction = (endpointId: number, id: string, action: string, force = false) =>
  call<void>("docker_container_action", { endpointId, id, action, force });
export const dockerContainerInspect = (endpointId: number, id: string) => call<unknown>("docker_container_inspect", { endpointId, id });
export const dockerImages = (endpointId: number) => call<ImageRow[]>("docker_images", { endpointId });
export const dockerImageInspect = (endpointId: number, name: string) => call<unknown>("docker_image_inspect", { endpointId, name });
export const dockerImageRemove = (endpointId: number, name: string) => call<void>("docker_image_remove", { endpointId, name });
export const dockerNetworks = (endpointId: number) => call<NetworkRow[]>("docker_networks", { endpointId });
export const dockerNetworkInspect = (endpointId: number, name: string) => call<unknown>("docker_network_inspect", { endpointId, name });
export const dockerNetworkRemove = (endpointId: number, name: string) => call<void>("docker_network_remove", { endpointId, name });
export const dockerListDirs = (endpointId: number, path: string) =>
  call<HostDirListing>("docker_list_dirs", { endpointId, path });
export const dockerComposeProjects = (endpointId: number) => call<ComposeProject[]>("docker_compose_projects", { endpointId });
export const dockerComposePreview = (path: string) => call<ComposePreview>("docker_compose_preview", { path });
export const dockerPickFolder = (title: string) => call<string | null>("docker_pick_folder", { title });
export const dockerComposeCreate = (endpointId: number, draft: ComposeProjectRequest) => call<void>("docker_compose_create", { endpointId, draft });
export const dockerComposeImport = (endpointId: number, path: string) => call<void>("docker_compose_import", { endpointId, path });
export const dockerComposeImportEngine = (endpointId: number, name: string) => call<void>("docker_compose_import_engine", { endpointId, name });
export const dockerComposeForget = (endpointId: number, id: number) => call<void>("docker_compose_forget", { endpointId, id });
export const dockerComposeAction = (endpointId: number, name: string, action: "build" | "up" | "down") =>
  call<void>("docker_compose_action", { endpointId, name, action });
export const dockerComposeRemove = (endpointId: number, name: string) => call<void>("docker_compose_remove", { endpointId, name });
export const dockerComposeFile = (endpointId: number, name: string) => call<string>("docker_compose_file", { endpointId, name });
export const dockerTaskStop = (taskId: number) => call<void>("docker_task_stop", { taskId });

export function dockerConnect(endpointId: number, events: Channel<DockerConnectEvent>) {
  return call<number>("docker_connect", { endpointId, events });
}

export function dockerContainerLogs(endpointId: number, id: string, events: Channel<DockerLogEvent>) {
  return call<number>("docker_container_logs", { endpointId, id, events });
}

export function dockerComposeLogs(endpointId: number, name: string, events: Channel<DockerLogEvent>) {
  return call<number>("docker_compose_logs", { endpointId, name, events });
}

export type DockerImageUpdateEvent = {
  type: "update";
  reference: string;
  state: "update" | "current" | "unknown";
};

export function dockerImageUpdates(endpointId: number, events: Channel<DockerImageUpdateEvent>) {
  return call<void>("docker_image_updates", { endpointId, events });
}
export const dockerImageUsers = (endpointId: number, reference: string) =>
  call<string[]>("docker_image_users", { endpointId, reference });

export function dockerImagePull(
  endpointId: number,
  reference: string,
  events: Channel<DockerPullEvent>,
  auth?: { username: string; password: string; remember: boolean },
) {
  return call<number>("docker_image_pull", {
    endpointId,
    reference,
    events,
    username: auth?.username ?? null,
    password: auth?.password ?? null,
    remember: auth?.remember ?? false,
  });
}

export type DockerExecEvent = { type: "state"; state: string } | { type: "closed"; code: string; params?: Record<string, unknown> };

export function dockerImageRun(endpointId: number, request: ContainerRunRequest) {
  return call<string>("docker_image_run", { endpointId, request });
}

export type ContainerUsage = {
  cpuPercent: number | null;
  memoryBytes: number;
  memoryLimit: number | null;
};

export function dockerContainerStats(endpointId: number, id: string) {
  return call<ContainerUsage>("docker_container_stats", { endpointId, id });
}

export function dockerExecOpen(endpointId: number, containerId: string, cols: number, rows: number, command: string, output: Channel<unknown>, events: Channel<DockerExecEvent>) {
  return call<number>("docker_exec_open", { endpointId, containerId, cols, rows, command, output, events });
}

export const dockerExecWrite = (id: number, data: number[]) => call<void>("docker_exec_write", { id, data });
export const dockerExecResize = (id: number, cols: number, rows: number) => call<void>("docker_exec_resize", { id, cols, rows });
export const dockerExecClose = (id: number) => call<void>("docker_exec_close", { id });
