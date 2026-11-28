export type SshAuth = "password" | "key" | "agent";

export type SshHost = {
  id: number;
  name: string;
  groupName: string | null;
  host: string;
  port: number;
  username: string;
  auth: SshAuth;
  keyPath: string | null;
  remember: boolean;
  startDir: string | null;
  term: string;
  sortOrder: number;
  jumps: number[];
  agentForward: boolean;
};

export type SshHostDraft = {
  id?: number;
  name: string;
  groupName?: string;
  host: string;
  port: number;
  username: string;
  auth: SshAuth;
  keyPath?: string;
  remember?: boolean;
  startDir?: string;
  term?: string;
  jumps?: number[];
  agentForward?: boolean;
};

export type SshSnippet = {
  id: number;
  title: string;
  body: string;
};

export type SshForwardKind = "local" | "remote" | "dynamic";

export type SshForward = {
  id: number;
  sessionId: number;
  kind: SshForwardKind | string;
  listenPort: number;
  targetHost: string;
  targetPort: number;
};

export type SshStatus = {
  enabled: boolean;
  keyringAvailable: boolean;
  agentAvailable: boolean;
  keepaliveSecs: number;
};

export type SshConfigPreview = {
  index: number;
  name: string;
  host: string;
  port: number;
  username: string;
  auth: string;
  keyPath: string | null;
  jumpNames: string[];
  warnings: string[];
  selected: boolean;
};

export type SshRemoteEntry = {
  name: string;
  path: string;
  kind: string;
  size: number;
};

export type SshRemoteListing = {
  path: string;
  parent: string | null;
  entries: SshRemoteEntry[];
};

export type SshSessionEvent =
  | { type: "state"; state: "connecting" | "verifying" | "authenticating" | "ready" }
  | {
      type: "hostkey";
      status: "unknown" | "changed";
      algorithm: string;
      fingerprint: string;
      previous?: string;
      source?: string;
      revoked?: boolean;
    }
  | {
      type: "prompt";
      promptId: number;
      kind: "password" | "passphrase" | "keyboard";
      canRemember?: boolean;
      remember?: boolean;
      hostName?: string;
      title?: string;
      instructions?: string;
      label?: string;
      echo?: boolean;
    }
  | { type: "notice"; code: string; params?: Record<string, unknown> }
  | { type: "cwd"; path: string }
  | { type: "closed"; code: string; params?: Record<string, unknown> };

export type TrustedHost = {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
};

export function isSshAuth(value: string): value is SshAuth {
  return value === "password" || value === "key" || value === "agent";
}
