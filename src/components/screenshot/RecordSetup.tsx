import type { RecordCursor } from "@/types/screenshot";

export type RecordSource = "off" | "on";
export type RecordMenu = "speaker" | "mic" | "camera" | "pointer";

export type RecordDevice = {
  id: string;
  name: string;
};

type Props = {
  barRef: React.RefObject<HTMLDivElement | null>;
  format: "mp4" | "gif";
  cursor: RecordCursor;
  speaker: RecordSource;
  mic: RecordSource;
  camera: RecordSource;
  speakerDevices: RecordDevice[];
  microphoneDevices: RecordDevice[];
  cameraDevices: RecordDevice[];
  speakerDevice: string;
  microphoneDevice: string;
  cameraDevice: string;
  menu: RecordMenu | null;
  onFormat: (format: "mp4" | "gif") => void;
  onCursor: (cursor: RecordCursor) => void;
  onSpeaker: (value: RecordSource) => void;
  onMic: (value: RecordSource) => void;
  onCamera: (value: RecordSource) => void;
  onSpeakerDevice: (id: string) => void;
  onMicrophoneDevice: (id: string) => void;
  onCameraDevice: (id: string) => void;
  onMenu: (menu: RecordMenu | null) => void;
  onStart: () => void;
};

export function RecordSetup({
  barRef,
  format,
  cursor,
  speaker,
  mic,
  camera,
  speakerDevices,
  microphoneDevices,
  cameraDevices,
  speakerDevice,
  microphoneDevice,
  cameraDevice,
  menu,
  onFormat,
  onCursor,
  onSpeaker,
  onMic,
  onCamera,
  onSpeakerDevice,
  onMicrophoneDevice,
  onCameraDevice,
  onMenu,
  onStart,
}: Props) {
  const { t } = useTranslation();
  const pictureOnly = format === "gif";
  const speakerShown: RecordSource = pictureOnly ? "off" : speaker;
  const micShown: RecordSource = pictureOnly ? "off" : mic;
  const speakerLabel = deviceName(speakerDevices, speakerDevice, t("zdx.label.screenshot.recordSystemAudio"));
  const micLabel = deviceName(microphoneDevices, microphoneDevice, t("zdx.label.screenshot.recordDefaultMic"));
  const cameraLabel = deviceName(cameraDevices, cameraDevice, t("zdx.label.screenshot.recordDefaultCamera"));
  const noDevice = t("zdx.text.screenshot.recordNoDevice");
  const highlightOn = cursor === "highlight" || cursor === "both";
  const clickOn = cursor === "click" || cursor === "both";
  const press = (event: React.MouseEvent<HTMLButtonElement>, action: () => void) => {
    event.preventDefault();
    event.stopPropagation();
    action();
  };

  return (
    <div
      ref={barRef}
      role="region"
      aria-label={t("zdx.label.screenshot.modeRecord")}
      className="w-80 rounded-xl p-3"
      style={{ background: "rgba(48, 48, 48, 0.96)", boxShadow: "0 16px 40px rgba(0, 0, 0, 0.35)" }}
    >
      <button
        type="button"
        className="h-9 w-full rounded-lg border-0 bg-[#3B82F6] text-sm font-medium text-white hover:bg-[#2563EB]"
        onMouseDown={(event) => press(event, onStart)}
      >
        {t("zdx.label.screenshot.recordStart")}
      </button>
      <div className="mt-3 flex items-center justify-between">
        <span className="text-xs text-white/80">{t("zdx.label.screenshot.recordFormat")}</span>
        <div role="radiogroup" aria-label={t("zdx.label.screenshot.recordFormat")} className="flex items-center gap-3">
          <FormatRadio label={t("zdx.label.screenshot.recordMp4")} selected={format === "mp4"} onMouseDown={(event) => press(event, () => onFormat("mp4"))} />
          <FormatRadio label={t("zdx.label.screenshot.recordGif")} selected={format === "gif"} onMouseDown={(event) => press(event, () => onFormat("gif"))} />
        </div>
      </div>
      <div className={pictureOnly ? "mt-2 flex items-start justify-center gap-2" : "mt-2 flex items-start gap-1"}>
        {!pictureOnly && (
          <>
            <SourceButton
              label={t("zdx.label.screenshot.recordSpeaker")}
              valueLabel={speakerShown === "on" ? speakerLabel : t("zdx.label.screenshot.recordDeviceOff")}
              icon="speaker"
              active={speakerShown === "on"}
              open={menu === "speaker"}
              onMouseDown={(event) => press(event, () => { onMenu(null); onSpeaker(speakerShown === "on" ? "off" : "on"); })}
              onMenuMouseDown={(event) => press(event, () => onMenu(menu === "speaker" ? null : "speaker"))}
            >
              {menu === "speaker" && (
                <DeviceMenu label={t("zdx.label.screenshot.recordSpeaker")} devices={speakerDevices} selected={speakerDevice} empty={noDevice} onSelect={(id) => { onSpeakerDevice(id); onSpeaker("on"); onMenu(null); }} />
              )}
            </SourceButton>
            <SourceButton
              label={t("zdx.label.screenshot.recordMic")}
              valueLabel={micShown === "on" ? micLabel : t("zdx.label.screenshot.recordDeviceOff")}
              icon="mic"
              active={micShown === "on"}
              open={menu === "mic"}
              onMouseDown={(event) => press(event, () => { onMenu(null); onMic(micShown === "on" ? "off" : "on"); })}
              onMenuMouseDown={(event) => press(event, () => onMenu(menu === "mic" ? null : "mic"))}
            >
              {menu === "mic" && (
                <DeviceMenu label={t("zdx.label.screenshot.recordMic")} devices={microphoneDevices} selected={microphoneDevice} empty={noDevice} onSelect={(id) => { onMicrophoneDevice(id); onMic("on"); onMenu(null); }} />
              )}
            </SourceButton>
          </>
        )}
        <SourceButton
          grow={!pictureOnly}
          label={t("zdx.label.screenshot.recordCamera")}
          valueLabel={camera === "on" ? cameraLabel : t("zdx.label.screenshot.recordDeviceOff")}
          icon="camera"
          active={camera === "on"}
          open={menu === "camera"}
          onMouseDown={(event) => press(event, () => { onMenu(null); onCamera(camera === "on" ? "off" : "on"); })}
          onMenuMouseDown={(event) => press(event, () => onMenu(menu === "camera" ? null : "camera"))}
        >
          {menu === "camera" && (
            <DeviceMenu label={t("zdx.label.screenshot.recordCamera")} devices={cameraDevices} selected={cameraDevice} empty={noDevice} onSelect={(id) => { onCameraDevice(id); onCamera("on"); onMenu(null); }} />
          )}
        </SourceButton>
        <SourceButton
          grow={!pictureOnly}
          label={t("zdx.label.screenshot.recordPointer")}
          valueLabel={cursorLabel(cursor, {
            off: t("zdx.label.screenshot.recordDeviceOff"),
            highlight: t("zdx.label.screenshot.recordPointerHighlight"),
            click: t("zdx.label.screenshot.recordPointerClick"),
            both: t("zdx.label.screenshot.recordPointerBoth"),
          })}
          icon="pointer"
          active={cursor !== "off"}
          open={menu === "pointer"}
          onMouseDown={(event) => press(event, () => onMenu(menu === "pointer" ? null : "pointer"))}
        >
          {menu === "pointer" && (
            <div role="group" aria-label={t("zdx.label.screenshot.recordPointer")} className="absolute top-full right-0 z-30 mt-1 min-w-36 rounded-lg py-1" style={{ background: "rgba(32, 32, 32, 0.98)", boxShadow: "0 8px 24px rgba(0, 0, 0, 0.35)" }}>
              <PointerCheck label={t("zdx.label.screenshot.recordPointerHighlight")} checked={highlightOn} onMouseDown={(event) => press(event, () => onCursor(cursorFrom(!highlightOn, clickOn)))} />
              <PointerCheck label={t("zdx.label.screenshot.recordPointerClick")} checked={clickOn} onMouseDown={(event) => press(event, () => onCursor(cursorFrom(highlightOn, !clickOn)))} />
            </div>
          )}
        </SourceButton>
      </div>
    </div>
  );
}

function deviceName(devices: RecordDevice[], id: string, fallback: string) {
  const match = devices.find((device) => device.id === id);
  const name = match?.name.trim();
  if (name) return name;
  return fallback;
}

function cursorLabel(cursor: RecordCursor, labels: Record<RecordCursor, string>) {
  return labels[cursor];
}

function cursorFrom(highlight: boolean, click: boolean): RecordCursor {
  if (highlight && click) return "both";
  if (highlight) return "highlight";
  if (click) return "click";
  return "off";
}

function PointerCheck({ label, checked, onMouseDown }: { label: string; checked: boolean; onMouseDown: (event: React.MouseEvent<HTMLButtonElement>) => void }) {
  return (
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={checked}
      className="flex w-full items-center gap-2 border-0 bg-transparent px-2.5 py-1.5 text-left text-xs whitespace-nowrap text-white hover:bg-white/10"
      onMouseDown={onMouseDown}
    >
      <span className={checked ? "flex h-4 w-4 shrink-0 items-center justify-center rounded bg-[#3B82F6]" : "h-4 w-4 shrink-0 rounded border border-solid border-white/50"}>
        {checked ? (
          <svg viewBox="0 0 16 16" className="h-3 w-3 text-white" aria-hidden="true">
            <path d="M3.5 8.2 6.4 11.1 12.5 4.8" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : null}
      </span>
      {label}
    </button>
  );
}

function FormatRadio({ label, selected, onMouseDown }: { label: string; selected: boolean; onMouseDown: (event: React.MouseEvent<HTMLButtonElement>) => void }) {
  return (
    <button type="button" role="radio" aria-checked={selected} className="flex items-center gap-1.5 border-0 bg-transparent p-0 text-xs text-white" onMouseDown={onMouseDown}>
      <span className={selected ? "flex h-3.5 w-3.5 items-center justify-center rounded-full border-2 border-solid border-[#3B82F6]" : "h-3.5 w-3.5 rounded-full border-2 border-solid border-white/60"}>
        {selected ? <span className="h-1.5 w-1.5 rounded-full bg-[#3B82F6]" /> : null}
      </span>
      {label}
    </button>
  );
}

function SourceButton({
  label,
  valueLabel,
  icon,
  active,
  open,
  grow = true,
  onMouseDown,
  onMenuMouseDown,
  children,
}: {
  label: string;
  valueLabel: string;
  icon: "speaker" | "mic" | "camera" | "pointer";
  active: boolean;
  open: boolean;
  grow?: boolean;
  onMouseDown: (event: React.MouseEvent<HTMLButtonElement>) => void;
  onMenuMouseDown?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  children?: React.ReactNode;
}) {
  const { t } = useTranslation();
  const className = active
    ? "relative flex w-full min-w-0 flex-col items-center gap-1 rounded-lg border-0 bg-[#3B82F6]/20 px-0.5 py-1 text-[#3B82F6]"
    : open
      ? "relative flex w-full min-w-0 flex-col items-center gap-1 rounded-lg border-0 bg-white/10 px-0.5 py-1 text-[#C7C7CC]"
      : "relative flex w-full min-w-0 flex-col items-center gap-1 rounded-lg border-0 bg-transparent px-0.5 py-1 text-[#C7C7CC] hover:bg-white/10";
  const menuClass = open
    ? "absolute top-2 left-[calc(50%+2px)] z-10 flex h-5 w-5 items-center justify-center rounded border-0 bg-white/15 p-0 text-[#C7C7CC]"
    : "absolute top-2 left-[calc(50%+2px)] z-10 flex h-5 w-5 items-center justify-center rounded border-0 bg-transparent p-0 text-[#8E8E93]";
  return (
    <div className={grow ? "relative min-w-0 flex-1" : "relative w-16"}>
      <button
        type="button"
        className={className}
        aria-pressed={onMenuMouseDown ? active : undefined}
        aria-haspopup={onMenuMouseDown ? undefined : "menu"}
        aria-expanded={onMenuMouseDown ? undefined : open}
        aria-label={t("zdx.label.screenshot.recordChoice", { name: label, value: valueLabel })}
        onMouseDown={onMouseDown}
      >
        <span className="relative flex h-6 w-7 items-center justify-center">
          <RecordIcon name={icon} muted={!active} active={active} />
          {!onMenuMouseDown && (
            <svg viewBox="0 0 8 8" className="absolute right-0 bottom-0 h-2 w-2 text-[#8E8E93]" aria-hidden="true">
              <path d="M1 2.5 4 5.5 7 2.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
        </span>
        <span className="text-center text-xs leading-4">{label}</span>
      </button>
      {onMenuMouseDown && (
        <button
          type="button"
          className={menuClass}
          aria-label={t("zdx.label.screenshot.recordDeviceMenu", { name: label })}
          aria-haspopup="menu"
          aria-expanded={open}
          onMouseDown={onMenuMouseDown}
        >
          <svg viewBox="0 0 8 8" className="h-2 w-2" aria-hidden="true">
            <path d="M1 2.5 4 5.5 7 2.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      )}
      {children}
    </div>
  );
}

function DeviceMenu({
  label,
  devices,
  selected,
  empty,
  onSelect,
}: {
  label: string;
  devices: RecordDevice[];
  selected: string;
  empty: string;
  onSelect: (id: string) => void;
}) {
  return (
    <ChoiceMenu label={label}>
      {devices.length === 0 ? (
        <p className="m-0 px-2 py-1.5 text-left text-xs text-[#8E8E93]">{empty}</p>
      ) : (
        devices.map((device) => (
          <ChoiceItem
            key={device.id || device.name}
            label={device.name}
            selected={selected === device.id}
            onMouseDown={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onSelect(device.id);
            }}
          />
        ))
      )}
    </ChoiceMenu>
  );
}

function ChoiceMenu({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div role="menu" aria-label={label} className="absolute top-full left-1/2 z-30 mt-1 max-h-48 w-64 -translate-x-1/2 overflow-y-auto rounded-lg p-1" style={{ background: "rgba(32, 32, 32, 0.98)", boxShadow: "0 8px 24px rgba(0, 0, 0, 0.35)" }}>
      {children}
    </div>
  );
}

function ChoiceItem({ label, selected, onMouseDown }: { label: string; selected: boolean; onMouseDown: (event: React.MouseEvent<HTMLButtonElement>) => void }) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      className={selected
        ? "flex w-full items-center justify-between gap-2 rounded-md border-0 bg-white/10 px-2 py-1.5 text-left text-xs text-[#3B82F6]"
        : "flex w-full items-center justify-between gap-2 rounded-md border-0 bg-transparent px-2 py-1.5 text-left text-xs text-white hover:bg-white/10"}
      onMouseDown={onMouseDown}
    >
      <span className="min-w-0 truncate">{label}</span>
      {selected ? (
        <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 shrink-0" aria-hidden="true">
          <path d="M3.5 8.2 6.4 11.1 12.5 4.8" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      ) : null}
    </button>
  );
}

function RecordIcon({ name, muted, active }: { name: "speaker" | "mic" | "camera" | "pointer"; muted: boolean; active: boolean }) {
  const className = active && !muted ? "h-5 w-5 text-[#3B82F6]" : "h-5 w-5 text-current";
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden="true">
      {name === "speaker" && (
        <>
          <path d="M5 9.5h2.8L12 6.2v11.6l-4.2-3.3H5z" fill="currentColor" />
          <path d="M15.2 9.2a3.6 3.6 0 0 1 0 5.6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )}
      {name === "mic" && (
        <>
          <rect x="9" y="3.5" width="6" height="10" rx="3" fill="none" stroke="currentColor" strokeWidth="1.6" />
          <path d="M7.5 11a4.5 4.5 0 0 0 9 0M12 15.5V19M9.2 19h5.6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )}
      {name === "camera" && (
        <>
          <rect x="3.2" y="7" width="12.2" height="10" rx="1.8" fill="none" stroke="currentColor" strokeWidth="1.6" />
          <path d="M15.4 11.2 20.2 8.6v6.8l-4.8-2.6z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
        </>
      )}
      {name === "pointer" && <path d="M7 4.2 7 18.4 10.5 14.7 13.8 20.4 15.9 19.3 12.6 13.6 17.6 13.2z" fill="currentColor" />}
      {muted && (
        <>
          <path d="M5 19.5 19.5 5" fill="none" stroke="#1C1C1C" strokeWidth="3.2" strokeLinecap="round" />
          <path d="M5 19.5 19.5 5" fill="none" stroke="#F5F5F7" strokeWidth="1.6" strokeLinecap="round" />
        </>
      )}
    </svg>
  );
}
