export type ToolAvailability = {
  clipboard: boolean | null;
  screenshot: boolean | null;
  ssh: boolean | null;
  docker: boolean | null;
};

export function homeForTools(flags: ToolAvailability): string | null {
  if (flags.clipboard === null || flags.screenshot === null || flags.ssh === null || flags.docker === null) return null;
  if (flags.clipboard) return "/clipboard";
  if (flags.screenshot) return "/screenshot";
  if (flags.ssh) return "/ssh";
  if (flags.docker) return "/docker";
  return "/settings";
}

export function redirectForClosedTool(path: string, flags: ToolAvailability): string | null {
  const home = homeForTools(flags);
  if (!home) return null;
  const closed =
    (path === "/clipboard" && flags.clipboard === false)
    || (path === "/screenshot" && flags.screenshot === false)
    || (path === "/ssh" && flags.ssh === false)
    || (path === "/docker" && flags.docker === false);
  if (!closed || path === home) return null;
  return home;
}
