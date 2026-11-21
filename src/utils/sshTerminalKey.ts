type TerminalKeyEvent = Pick<KeyboardEvent, "type" | "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

export type SshTerminalShortcut = "leave" | "copy" | "paste" | "find";

export function sshTerminalShortcut(event: TerminalKeyEvent): SshTerminalShortcut | null {
  if (event.type !== "keydown") return null;
  const key = event.key.toLowerCase();
  const command = event.metaKey && !event.ctrlKey && !event.altKey;
  const shiftCtrl = event.ctrlKey && event.shiftKey && !event.metaKey && !event.altKey;
  if ((command || shiftCtrl) && event.shiftKey && key === "e") return "leave";
  if ((command && key === "c") || (shiftCtrl && key === "c")) return "copy";
  if ((command && key === "v") || (shiftCtrl && key === "v")) return "paste";
  if ((command || (event.ctrlKey && !event.shiftKey)) && key === "f") return "find";
  return null;
}
