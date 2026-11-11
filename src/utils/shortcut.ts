const MAC_MODIFIER_ORDER = ["Control", "Alt", "Shift", "CommandOrControl", "Command"] as const;
const OTHER_MODIFIER_ORDER = ["CommandOrControl", "Control", "Command", "Alt", "Shift"] as const;

const KEY_TOKENS: Record<string, string> = {
  Backquote: "Backquote",
  Backslash: "Backslash",
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
  Comma: "Comma",
  Equal: "Equal",
  Minus: "Minus",
  Period: "Period",
  Quote: "Quote",
  Semicolon: "Semicolon",
  Slash: "Slash",
  Space: "Space",
  Tab: "Tab",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Escape: "Escape",
  Home: "Home",
  End: "End",
  Insert: "Insert",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Numpad0: "Num0",
  Numpad1: "Num1",
  Numpad2: "Num2",
  Numpad3: "Num3",
  Numpad4: "Num4",
  Numpad5: "Num5",
  Numpad6: "Num6",
  Numpad7: "Num7",
  Numpad8: "Num8",
  Numpad9: "Num9",
  NumpadAdd: "NumAdd",
  NumpadSubtract: "NumSubtract",
  NumpadMultiply: "NumMultiply",
  NumpadDivide: "NumDivide",
  NumpadDecimal: "NumDecimal",
  NumpadEnter: "NumEnter",
  NumpadEqual: "NumEqual",
};

export interface ShortcutKeys {
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export function keyToken(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F(?:[1-9]|1\d|2[0-4])$/.test(code)) return code;
  return KEY_TOKENS[code] ?? null;
}

export const DEFAULT_PIN_SHORTCUT = "CommandOrControl+P";
export const DEFAULT_DELETE_SHORTCUT = "CommandOrControl+Backspace";

export function shortcutFromEvent(event: ShortcutKeys): string | null {
  const key = keyToken(event.code);
  if (!key) return null;
  const modifiers: string[] = [];
  if (event.metaKey) modifiers.push("CommandOrControl");
  if (event.ctrlKey) modifiers.push("Control");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (modifiers.length === 0) return null;
  return [...modifiers, key].join("+");
}

export function isApplePlatform(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
}

export function eventMatchesShortcut(event: ShortcutKeys, shortcut: string, macos = isApplePlatform()): boolean {
  const expected = describeShortcut(shortcut, macos);
  const key = keyToken(event.code);
  if (!expected || !key || expected.key.toLowerCase() !== key.toLowerCase()) return false;
  return expected.command === event.metaKey && expected.control === event.ctrlKey && expected.alt === event.altKey && expected.shift === event.shiftKey;
}

export function formatShortcut(shortcut: string, macos: boolean): string {
  const parts = shortcut.split("+").map((part) => part.trim()).filter((part) => part.length > 0);
  const order = macos ? MAC_MODIFIER_ORDER : OTHER_MODIFIER_ORDER;
  const modifiers = order.flatMap((name) => parts.filter((part) => sameModifier(part, name)).map((part) => modifierLabel(part, macos)));
  const keys = parts.filter((part) => !isModifier(part)).map((part) => keyLabel(part, macos));
  const labels = [...modifiers, ...keys];
  return macos ? labels.join("") : labels.join("+");
}

function sameModifier(part: string, name: string): boolean {
  return canonicalModifier(part) === name;
}

function canonicalModifier(part: string): string | null {
  switch (part.toLowerCase()) {
    case "control":
    case "ctrl":
      return "Control";
    case "alt":
    case "option":
      return "Alt";
    case "shift":
      return "Shift";
    case "commandorcontrol":
    case "commandorctrl":
    case "cmdorctrl":
    case "cmdorcontrol":
      return "CommandOrControl";
    case "command":
    case "cmd":
    case "super":
      return "Command";
    default:
      return null;
  }
}

function describeShortcut(shortcut: string, macos: boolean): { command: boolean; control: boolean; alt: boolean; shift: boolean; key: string } | null {
  let command = false;
  let control = false;
  let alt = false;
  let shift = false;
  let key = "";
  for (const part of shortcut.split("+").map((item) => item.trim()).filter((item) => item.length > 0)) {
    const modifier = canonicalModifier(part);
    if (modifier === "CommandOrControl") {
      if (macos) command = true;
      else control = true;
    } else if (modifier === "Command") command = true;
    else if (modifier === "Control") control = true;
    else if (modifier === "Alt") alt = true;
    else if (modifier === "Shift") shift = true;
    else if (key.length > 0) return null;
    else key = part;
  }
  if (key.length === 0 || (!command && !control && !alt && !shift)) return null;
  return { command, control, alt, shift, key };
}

function isModifier(part: string): boolean {
  return canonicalModifier(part) !== null;
}

function modifierLabel(part: string, macos: boolean): string {
  const name = canonicalModifier(part);
  if (macos) {
    if (name === "Control") return "⌃";
    if (name === "Alt") return "⌥";
    if (name === "Shift") return "⇧";
    return "⌘";
  }
  if (name === "Alt") return "Alt";
  if (name === "Shift") return "Shift";
  if (name === "Command") return "Win";
  return "Ctrl";
}

function keyLabel(part: string, macos: boolean): string {
  const labels: Record<string, string> = macos
    ? { Up: "↑", Down: "↓", Left: "←", Right: "→", Enter: "↩", Tab: "⇥", Backspace: "⌫", Delete: "⌦", Escape: "⎋", Space: "␣" }
    : { Up: "Up", Down: "Down", Left: "Left", Right: "Right" };
  return labels[part] ?? part;
}
