import type { SshSessionEvent } from "@/types/ssh";

export type DockerKind = "local" | "ssh";

export type DockerStatus = {
  enabled: boolean;
  imageUpdates: boolean;
};

export type DockerEndpoint = {
  id: number;
  name: string;
  kind: DockerKind;
  socketPath: string | null;
  sshHostId: number | null;
  remoteSocket: string | null;
  host: string | null;
  port: number | null;
  source: string;
  sortOrder: number;
};

export type DockerEndpointDraft = {
  id?: number;
  name: string;
  kind: DockerKind;
  socketPath?: string;
  sshHostId?: number;
  remoteSocket?: string;
  host?: string;
  port?: number;
};

export type DockerSocket = {
  path: string;
  hint: string;
};

export type DockerContextPreview = {
  index: number;
  name: string;
  kind: string;
  detail: string;
  warning: string | null;
  importable: boolean;
  current: boolean;
};

export type EngineSummary = {
  name: string;
  hostname: string;
  version: string;
  os: string;
  arch: string;
  cpus: number;
  memory: number;
  containers: number;
  images: number;
  socket: string | null;
};

export type ContainerRow = {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  ports: string[];
  created: number;
};

export type ContainerRunRequest = {
  image: string;
  name: string;
  restart: string;
  network: string;
  ports: string;
  mounts: { kind: "bind" | "volume"; source: string; target: string; readOnly: boolean }[];
  env: { key: string; value: string }[];
  command: string;
  workdir: string;
};

export type ImageRow = {
  id: string;
  tags: string[];
  size: number;
  created: number;
};

export type NetworkRow = {
  id: string;
  name: string;
  driver: string;
  scope: string;
  subnet: string;
  gateway: string;
  ipv6: boolean;
};

export type ComposeService = {
  name: string;
  image: string;
  ports: string[];
};

export type ComposeProjectRequest = {
  name: string;
  directory: string;
  source: "file" | "yaml";
  filePath?: string;
  yaml?: string;
  start?: boolean;
};

export type ComposePreview = {
  name: string;
  configPath: string;
  workingDir: string;
  services: ComposeService[];
};

export type ComposeProject = ComposePreview & {
  id: number | null;
  running: number;
  total: number;
  imported: boolean;
  origin: "engine" | "file" | "both";
};

export type HostDirEntry = {
  name: string;
  path: string;
};

export type HostDirListing = {
  path: string;
  parent: string | null;
  entries: HostDirEntry[];
};

export type DockerConnectEvent =
  | Exclude<SshSessionEvent, { type: "ready" | "cwd" }>
  | { type: "ready"; summary: EngineSummary }
  | { type: "closed"; code: string; params?: Record<string, unknown> };

export type DockerLogEvent =
  | { type: "data"; stream: "stdout" | "stderr"; text: string }
  | { type: "closed"; code: string; params?: Record<string, unknown> };

export type DockerPullEvent =
  | { type: "progress"; status: string; id: string; current: number; total: number }
  | { type: "closed"; code: string; params?: Record<string, unknown> };
