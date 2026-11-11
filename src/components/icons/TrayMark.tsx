export const TRAY_ICONS = [
  { id: "board", label: "zdx.label.system.trayIconBoard" },
  { id: "clip", label: "zdx.label.system.trayIconClip" },
  { id: "mark", label: "zdx.label.system.trayIconMark" },
  { id: "pages", label: "zdx.label.system.trayIconPages" },
  { id: "pin", label: "zdx.label.system.trayIconPin" },
] as const;

export type TrayIconId = (typeof TRAY_ICONS)[number]["id"];

export function isTrayIcon(value: unknown): value is TrayIconId {
  return typeof value === "string" && TRAY_ICONS.some((icon) => icon.id === value);
}

function Glyph({ children }: { children: ReactNode }) {
  return (
    <svg width={28} height={28} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      {children}
    </svg>
  );
}

export function TrayMarkGlyph({ id }: { id: TrayIconId }) {
  if (id === "clip") {
    return (
      <Glyph>
        <path fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" d="M10 14.5V8.2a2 2 0 0 1 4 0v8.2a3.2 3.2 0 0 1-6.4 0V9.2" />
      </Glyph>
    );
  }
  if (id === "mark") {
    return (
      <Glyph>
        <path d="M7.2 18V6.2h4.2a3.1 3.1 0 0 1 0 6.2H7.2" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinejoin="round" />
      </Glyph>
    );
  }
  if (id === "pages") {
    return (
      <Glyph>
        <rect x="4.2" y="7.2" width="11" height="12.2" rx="1.6" opacity="0.45" />
        <rect x="8.4" y="4.6" width="11" height="12.2" rx="1.6" />
      </Glyph>
    );
  }
  if (id === "pin") {
    return (
      <Glyph>
        <circle cx="12" cy="7.2" r="2.3" />
        <rect x="10.9" y="8.8" width="2.2" height="9.4" rx="1" />
        <rect x="7.4" y="11.2" width="9.2" height="2" rx="1" />
      </Glyph>
    );
  }
  return (
    <Glyph>
      <path fill="none" stroke="currentColor" strokeWidth="1.8" d="M8 8.2h8.2a1.4 1.4 0 0 1 1.4 1.4V18a1.4 1.4 0 0 1-1.4 1.4H8A1.4 1.4 0 0 1 6.6 18V9.6A1.4 1.4 0 0 1 8 8.2Z" />
      <rect x="9.2" y="4.8" width="5.6" height="3.2" rx="1" />
      <path fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" d="M9.2 13.2h5.6M9.2 16.1h3.6" />
    </Glyph>
  );
}
