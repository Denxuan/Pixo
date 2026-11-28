import type { SshRequestError } from "@/api/ssh";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function sshErrorText(t: Translate, error: Pick<SshRequestError, "code" | "params">): string {
  const key = `zdx.text.ssh.error.${error.code}`;
  const translated = t(key, error.params);
  return translated === key ? t("zdx.text.ssh.error.unknown") : translated;
}

export function sshPhaseText(t: Translate, phase: string): string {
  switch (phase) {
    case "connecting":
      return t("zdx.text.ssh.connecting");
    case "verifying":
      return t("zdx.text.ssh.verifying");
    case "authenticating":
      return t("zdx.text.ssh.authenticating");
    case "ready":
      return t("zdx.text.ssh.ready");
    case "closed":
      return t("zdx.text.ssh.closed");
    case "idle":
      return t("zdx.text.ssh.restored");
    default:
      return "";
  }
}

export function terminalBytes(message: unknown): Uint8Array | null {
  if (message instanceof ArrayBuffer) return new Uint8Array(message);
  if (ArrayBuffer.isView(message)) {
    return new Uint8Array(message.buffer, message.byteOffset, message.byteLength);
  }
  if (Array.isArray(message) && message.every((item) => typeof item === "number")) {
    return Uint8Array.from(message);
  }
  return null;
}
