export type SshAuth = "password" | "key";

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
};

export type SshStatus = {
  enabled: boolean;
  keyringAvailable: boolean;
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
  | { type: "prompt"; promptId: number; kind: "password" | "passphrase"; canRemember?: boolean; remember?: boolean }
  | { type: "notice"; code: string; params?: Record<string, unknown> }
  | { type: "cwd"; path: string }
  | { type: "closed"; code: string; params?: Record<string, unknown> };

export function isSshAuth(value: string): value is SshAuth {
  return value === "password" || value === "key";
}
