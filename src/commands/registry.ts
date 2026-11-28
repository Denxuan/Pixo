export type CommandId =
  | "screenshot.capture"
  | "screenshot.record"
  | "clipboard.panel"
  | "clipboard.pause"
  | "clipboard.resume"
  | "settings.open"
  | "settings.data";

export type CommandContext = {
  query: string;
  clipboardOn: boolean;
  clipboardPaused: boolean;
  screenshotOn: boolean;
};

export type CommandDef = {
  id: CommandId;
  titleKey: string;
  detailKey: string;
  keywords: string[];
  when: (context: CommandContext) => boolean;
};

const COMMANDS: CommandDef[] = [
  {
    id: "screenshot.capture",
    titleKey: "zdx.label.launcher.capture",
    detailKey: "zdx.text.launcher.capture",
    keywords: ["截图", "screenshot", "capture", "shot"],
    when: (context) => context.screenshotOn,
  },
  {
    id: "screenshot.record",
    titleKey: "zdx.label.launcher.record",
    detailKey: "zdx.text.launcher.record",
    keywords: ["录屏", "record", "recording"],
    when: (context) => context.screenshotOn,
  },
  {
    id: "clipboard.panel",
    titleKey: "zdx.label.launcher.panel",
    detailKey: "zdx.text.launcher.panel",
    keywords: ["剪贴板", "clipboard", "panel", "历史", "history"],
    when: (context) => context.clipboardOn,
  },
  {
    id: "clipboard.pause",
    titleKey: "zdx.label.launcher.pause",
    detailKey: "zdx.text.launcher.pause",
    keywords: ["暂停", "pause"],
    when: (context) => context.clipboardOn && !context.clipboardPaused,
  },
  {
    id: "clipboard.resume",
    titleKey: "zdx.label.launcher.resume",
    detailKey: "zdx.text.launcher.resume",
    keywords: ["继续", "resume"],
    when: (context) => context.clipboardOn && context.clipboardPaused,
  },
  {
    id: "settings.open",
    titleKey: "zdx.label.launcher.settings",
    detailKey: "zdx.text.launcher.settings",
    keywords: ["设置", "settings"],
    when: () => true,
  },
  {
    id: "settings.data",
    titleKey: "zdx.label.launcher.data",
    detailKey: "zdx.text.launcher.data",
    keywords: ["数据", "隐私", "导出", "data", "privacy", "export"],
    when: () => true,
  },
];

function matches(query: string, keywords: string[]) {
  return keywords.some((word) => word.toLowerCase().startsWith(query));
}

export function arrangeLauncherHits<T>(query: string, commands: T[], objects: T[]): T[] {
  return query.trim() ? [...objects, ...commands] : [...commands, ...objects];
}

export function matchCommands(context: CommandContext): CommandDef[] {
  const query = context.query.trim().toLowerCase();
  const available = COMMANDS.filter((command) => command.when(context));
  if (!query) {
    return available.filter((command) => command.id !== "settings.open" && command.id !== "settings.data");
  }
  return available.filter((command) => matches(query, command.keywords));
}
