let held = false;

export function holdPermissionPrompt() {
  held = true;
}

export function permissionPromptHeld() {
  return held;
}
