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
    <svg width={28} height={28} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
      {children}
    </svg>
  );
}

export function TrayMarkGlyph({ id }: { id: TrayIconId }) {
  if (id === "clip") {
    return (
      <Glyph>
        <path
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M6.8 7.6V14.6A4.8 4.8 0 0 1 16.4 14.6V6.6A3.1 3.1 0 0 0 10.2 6.6V14.8A1.5 1.5 0 0 1 13.2 14.8V9.6"
        />
      </Glyph>
    );
  }
  if (id === "mark") {
    return (
      <Glyph>
        <path
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M6.9 19.6V4.4H12.1a5 5 0 0 1 0 10H6.9"
        />
      </Glyph>
    );
  }
  if (id === "pages") {
    return (
      <Glyph>
        <path
          strokeWidth="2"
          strokeLinejoin="round"
          d="M7.8 15.2H6.5A2 2 0 0 1 4.5 13.2V6A2 2 0 0 1 6.5 4H13A2 2 0 0 1 15 6V7.4"
        />
        <rect x="8.8" y="8.4" width="10.5" height="11.2" rx="2" strokeWidth="2" />
        <path strokeWidth="1.6" strokeLinecap="round" d="M11.4 12.6h5M11.4 15.5h2.8" />
      </Glyph>
    );
  }
  if (id === "pin") {
    return (
      <Glyph>
        <rect x="6.9" y="3.6" width="10.2" height="5.6" rx="2.8" strokeWidth="2" />
        <path strokeWidth="2" strokeLinecap="round" d="M12 9.2v11.2" />
      </Glyph>
    );
  }
  return (
    <Glyph>
      <path
        strokeWidth="2"
        strokeLinejoin="round"
        d="M8.8 6.4H7.2A2.2 2.2 0 0 0 5 8.6V17.8A2.2 2.2 0 0 0 7.2 20H16.8A2.2 2.2 0 0 0 19 17.8V8.6A2.2 2.2 0 0 0 16.8 6.4H15.2"
      />
      <rect x="8.8" y="3.6" width="6.4" height="4.6" rx="1.5" strokeWidth="2" />
      <path strokeWidth="1.65" strokeLinecap="round" d="M8.3 12.2h7.4M8.3 15.4h4.9" />
    </Glyph>
  );
}
