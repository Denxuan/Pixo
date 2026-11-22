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
  ImageRow,
  NetworkRow,
  ComposePreview,
  ComposeProject,
  VolumeRow,
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
export const dockerEndpoints = () => call<DockerEndpoint[]>("docker_endpoints_list");
export const dockerSaveEndpoint = (endpoint: DockerEndpointDraft) => call<DockerEndpoint>("docker_endpoint_save", { endpoint });
export const dockerDeleteEndpoint = (id: number) => call<void>("docker_endpoint_delete", { id });
export const dockerSockets = () => call<DockerSocket[]>("docker_local_sockets");
export const dockerContextPreview = () => call<DockerContextPreview[]>("docker_contexts_preview");
export const dockerContextImport = (indexes: number[]) => call<DockerEndpoint[]>("docker_contexts_import", { indexes });
export const dockerPickFile = (title: string) => call<string | null>("docker_pick_file", { title });
export const dockerDisconnect = (endpointId: number) => call<void>("docker_disconnect", { endpointId });
export const dockerHostKeyDecide = (linkId: number, accept: boolean) => call<void>("docker_hostkey_decide", { linkId, accept });
export const dockerAnswer = (linkId: number, promptId: number, secret: string, remember: boolean, cancel: boolean) =>
  call<void>("docker_answer", { linkId, promptId, secret, remember, cancel });
export const dockerSummary = (endpointId: number) => call<EngineSummary>("docker_summary", { endpointId });
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
export const dockerVolumes = (endpointId: number) => call<VolumeRow[]>("docker_volumes", { endpointId });
export const dockerVolumeInspect = (endpointId: number, name: string) => call<unknown>("docker_volume_inspect", { endpointId, name });
export const dockerVolumeRemove = (endpointId: number, name: string) => call<void>("docker_volume_remove", { endpointId, name });
export const dockerComposeProjects = (endpointId: number) => call<ComposeProject[]>("docker_compose_projects", { endpointId });
export const dockerComposePreview = (path: string) => call<ComposePreview>("docker_compose_preview", { path });
export const dockerComposeImport = (endpointId: number, path: string) => call<void>("docker_compose_import", { endpointId, path });
export const dockerComposeImportEngine = (endpointId: number, name: string) => call<void>("docker_compose_import_engine", { endpointId, name });
export const dockerComposeForget = (endpointId: number, id: number) => call<void>("docker_compose_forget", { endpointId, id });
export const dockerTaskStop = (taskId: number) => call<void>("docker_task_stop", { taskId });

export function dockerConnect(endpointId: number, events: Channel<DockerConnectEvent>) {
  return call<number>("docker_connect", { endpointId, events });
}

export function dockerContainerLogs(endpointId: number, id: string, events: Channel<DockerLogEvent>) {
  return call<number>("docker_container_logs", { endpointId, id, events });
}

export function dockerImagePull(endpointId: number, reference: string, events: Channel<DockerPullEvent>) {
  return call<number>("docker_image_pull", { endpointId, reference, events });
}
