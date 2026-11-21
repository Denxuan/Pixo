import type { InputRef } from "antd";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { clipboardIgnoreNext } from "@/api/clipboard";
import { SshRequestError, sshConnect, sshDisconnect, sshPickUpload, sshReleaseUpload, sshResize, sshTransferCancel, sshTransferDecide, sshTransferStart, sshWrite } from "@/api/ssh";
import TerminalAppearance from "@/components/ssh/TerminalAppearance";
import { Channel } from "@tauri-apps/api/core";
import type { SshHost, SshSessionEvent } from "@/types/ssh";
import { registerSshDropTarget } from "@/utils/sshDrop";
import { formatBytes, reduceTransfer, transferClosesItself, transferPercent, type SshTransferEvent, type TransferRow } from "@/utils/sshTransfer";
import { useSshAppearanceStore } from "@/stores/sshAppearance";
import { sshTerminalShortcut } from "@/utils/sshTerminalKey";
import { isDarkTheme } from "@/utils/theme";
import { isDesktop } from "@/utils/desktop";
import { useSettingsStore } from "@/stores/settings";
import { resolvePalette, sshThemes, terminalFontFamily, toTerminalTheme, type SshThemeId } from "@/utils/sshAppearance";
import { sshErrorText, sshPhaseText, terminalBytes } from "@/utils/sshMessage";

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const subscribeScheme = (onChange: () => void) => {
  colorScheme.addEventListener("change", onChange);
  return () => colorScheme.removeEventListener("change", onChange);
};

export default function TerminalView({
  host,
  connectKey,
  onSession,
  onEvent,
  onReconnect,
}: {
  host: SshHost;
  connectKey: number;
  onSession: (sessionId: number) => void;
  onEvent: (sessionId: number, event: SshSessionEvent) => void;
  onReconnect: () => void;
}) {
  const { t, i18n } = useTranslation();
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(subscribeScheme, () => colorScheme.matches);
  const dark = isDarkTheme(mode, systemDark);
  const themeId = useSshAppearanceStore((state) => state.themeId);
  const setThemeId = useSshAppearanceStore((state) => state.setThemeId);
  const customPalette = useSshAppearanceStore((state) => state.custom);
  const fontId = useSshAppearanceStore((state) => state.fontId);
  const customFont = useSshAppearanceStore((state) => state.customFont);
  const fontSize = useSshAppearanceStore((state) => state.fontSize);
  const lineHeight = useSshAppearanceStore((state) => state.lineHeight);
  const letterSpacing = useSshAppearanceStore((state) => state.letterSpacing);
  const cursorStyle = useSshAppearanceStore((state) => state.cursorStyle);
  const cursorBlink = useSshAppearanceStore((state) => state.cursorBlink);
  const contrast = useSshAppearanceStore((state) => state.contrast);
  const scrollback = useSshAppearanceStore((state) => state.scrollback);
  const palette = resolvePalette(themeId, customPalette, dark);
  const fontFamily = terminalFontFamily(fontId, customFont);
  const rootRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<InputRef>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef(0);
  const phaseRef = useRef("connecting");
  const darkRef = useRef(dark);
  const onEventRef = useRef(onEvent);
  const onSessionRef = useRef(onSession);
  const translateRef = useRef(t);
  darkRef.current = dark;
  onEventRef.current = onEvent;
  onSessionRef.current = onSession;
  translateRef.current = t;
  const [phase, setPhase] = useState("connecting");
  const [closed, setClosed] = useState("");
  const [query, setQuery] = useState("");
  const [reader, setReader] = useState(false);
  const [appearanceOpen, setAppearanceOpen] = useState(false);
  const [cwd, setCwd] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [notice, setNotice] = useState("");
  const [tasks, setTasks] = useState<TransferRow[]>([]);
  const [conflicts, setConflicts] = useState<{ taskId: number; name: string }[]>([]);
  const [applyAll, setApplyAll] = useState(true);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const uploadRef = useRef<(token: string) => void>(() => undefined);
  const dismissTimers = useRef(new Map<number, number>());
  const searchRef = useRef<SearchAddon | null>(null);

  useEffect(() => {
    phaseRef.current = phase;
    const term = termRef.current;
    if (term) term.options.disableStdin = phase !== "ready";
  }, [phase]);

  useEffect(() => {
    const term = termRef.current;
    const fit = fitRef.current;
    if (!term || !fit) return;
    term.options.theme = toTerminalTheme(resolvePalette(themeId, customPalette, dark));
    term.options.fontFamily = fontFamily;
    term.options.fontSize = fontSize;
    term.options.lineHeight = lineHeight;
    term.options.letterSpacing = letterSpacing;
    term.options.cursorStyle = cursorStyle;
    term.options.cursorBlink = cursorBlink;
    term.options.minimumContrastRatio = contrast ? 4.5 : 1;
    term.options.scrollback = scrollback;
    term.options.drawBoldTextInBrightColors = true;
    fit.fit();
    const sessionId = sessionRef.current;
    if (sessionId) void sshResize(sessionId, term.cols, term.rows).catch(() => undefined);
  }, [contrast, cursorBlink, cursorStyle, customPalette, dark, fontFamily, fontSize, letterSpacing, lineHeight, scrollback, themeId]);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    let cancelled = false;
    let sessionId = 0;
    const current = useSshAppearanceStore.getState();
    const initialPalette = resolvePalette(current.themeId, current.custom, darkRef.current);
    const term = new Terminal({
      cursorBlink: current.cursorBlink,
      cursorStyle: current.cursorStyle,
      fontSize: current.fontSize,
      fontFamily: terminalFontFamily(current.fontId, current.customFont),
      lineHeight: current.lineHeight,
      letterSpacing: current.letterSpacing,
      minimumContrastRatio: current.contrast ? 4.5 : 1,
      drawBoldTextInBrightColors: true,
      scrollback: current.scrollback,
      theme: toTerminalTheme(initialPalette),
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.open(element);
    const fitNow = () => fit.fit();
    fitNow();
    requestAnimationFrame(fitNow);
    termRef.current = term;
    fitRef.current = fit;
    searchRef.current = search;
    const output = new Channel<unknown>((message) => {
      const bytes = terminalBytes(message);
      if (bytes) term.write(bytes);
    });
    const events = new Channel<SshSessionEvent>();
    const pending: SshSessionEvent[] = [];
    const handle = (id: number, event: SshSessionEvent) => {
      if (event.type === "cwd") setCwd(event.path);
      if (event.type === "state") {
        setPhase(event.state);
        if (event.state === "ready") setClosed("");
      }
      if (event.type === "closed") {
        setPhase("closed");
        setClosed(sshErrorText(translateRef.current, { code: event.code, params: event.params ?? {} }));
      }
      onEventRef.current(id, event);
    };
    events.onmessage = (event) => {
      if (!sessionId) pending.push(event);
      else handle(sessionId, event);
    };
    term.onData((data) => {
      if (phaseRef.current !== "ready" || sessionId === 0) return;
      void sshWrite(sessionId, new TextEncoder().encode(data));
    });
    term.onBinary((data) => {
      if (phaseRef.current !== "ready" || sessionId === 0) return;
      const bytes = Uint8Array.from(data, (char) => char.charCodeAt(0) & 255);
      void sshWrite(sessionId, bytes);
    });
    term.attachCustomKeyEventHandler((event) => {
      const action = sshTerminalShortcut(event);
      if (!action) return true;
      event.preventDefault();
      if (action === "leave") term.blur();
      if (action === "copy") void copySelection(term);
      if (action === "paste") void navigator.clipboard.readText().then((text) => paste(term, text, translateRef.current)).catch(() => undefined);
      if (action === "find") searchInputRef.current?.focus();
      return false;
    });
    const showMenu = (event: MouseEvent) => {
      event.preventDefault();
      setMenuAt({ x: event.clientX, y: event.clientY });
    };
    element.addEventListener("contextmenu", showMenu);
    const observer = new ResizeObserver(() => {
      fit.fit();
      if (sessionId) void sshResize(sessionId, term.cols, term.rows).catch(() => undefined);
    });
    observer.observe(element);
    void sshConnect(host.id, term.cols || 80, term.rows || 24, output, events).then((id) => {
      if (cancelled) {
        void sshDisconnect(id);
        return;
      }
      sessionId = id;
      sessionRef.current = id;
      onSessionRef.current(id);
      for (const event of pending) handle(id, event);
      void sshResize(id, term.cols, term.rows).catch(() => undefined);
    }).catch((error) => {
      if (cancelled) return;
      setPhase("closed");
      setClosed(error instanceof SshRequestError ? sshErrorText(translateRef.current, error) : translateRef.current("zdx.text.ssh.error.unknown"));
    });
    return () => {
      cancelled = true;
      element.removeEventListener("contextmenu", showMenu);
      observer.disconnect();
      if (sessionId) void sshDisconnect(sessionId);
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
      searchRef.current = null;
    };
  }, [connectKey, host.id]);

  async function beginUpload(token: string) {
    const sessionId = sessionRef.current;
    if (!sessionId || phaseRef.current !== "ready") {
      void sshReleaseUpload(token);
      setNotice(t("zdx.text.ssh.dropNotReady"));
      return;
    }
    setNotice("");
    const progress = new Channel<SshTransferEvent>();
    progress.onmessage = (event) => {
      if (event.type === "conflict") {
        setConflicts((current) => current.some((item) => item.taskId === event.taskId && item.name === event.name)
          ? current
          : [...current, { taskId: event.taskId, name: event.name }]);
        return;
      }
      setTasks((current) => reduceTransfer(current, event));
    };
    try {
      await sshTransferStart(sessionId, token, progress);
    } catch (error) {
      setNotice(error instanceof SshRequestError ? sshErrorText(t, error) : t("zdx.text.ssh.error.unknown"));
    }
  }
  uploadRef.current = (token) => {
    void beginUpload(token);
  };

  useEffect(() => {
    for (const task of tasks) {
      if (!transferClosesItself(task) || dismissTimers.current.has(task.taskId)) continue;
      const taskId = task.taskId;
      dismissTimers.current.set(taskId, window.setTimeout(() => {
        dismissTimers.current.delete(taskId);
        setTasks((current) => current.filter((item) => item.taskId !== taskId));
      }, 1000));
    }
  }, [tasks]);

  useEffect(() => {
    const timers = dismissTimers.current;
    return () => {
      for (const timer of timers.values()) window.clearTimeout(timer);
      timers.clear();
    };
  }, []);

  useEffect(() => {
    return registerSshDropTarget({
      rect: () => {
        const element = rootRef.current;
        if (!element) return null;
        const box = element.getBoundingClientRect();
        if (box.width <= 0 || box.height <= 0) return null;
        return box;
      },
      ready: () => phaseRef.current === "ready" && sessionRef.current !== 0,
      hover: setDragOver,
      upload: (token) => uploadRef.current(token),
      reject: () => setNotice(translateRef.current("zdx.text.ssh.dropNotReady")),
    });
  }, []);

  async function pickUpload(kind: "files" | "folder") {
    try {
      const picked = await sshPickUpload(kind, t(kind === "folder" ? "zdx.label.ssh.uploadFolder" : "zdx.label.ssh.uploadFiles"));
      if (picked) await beginUpload(picked.token);
    } catch (error) {
      setNotice(error instanceof SshRequestError ? sshErrorText(t, error) : t("zdx.text.ssh.error.unknown"));
    }
  }

  const conflict = conflicts[0];

  return (
    <div ref={rootRef} className="flex h-full min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-2 px-3 py-2">
        <span className="text-xs" style={{ opacity: 0.7 }}>{closed || sshPhaseText(t, phase)}</span>
        <span className="max-w-72 truncate text-xs" style={{ opacity: 0.7 }}>{cwd ? t("zdx.text.ssh.uploadTarget", { path: cwd }) : t("zdx.text.ssh.uploadTargetPending")}</span>
        <Dropdown
          menu={{
            items: [
              { key: "files", label: t("zdx.label.ssh.uploadFiles"), disabled: phase !== "ready", onClick: () => void pickUpload("files") },
              { key: "folder", label: t("zdx.label.ssh.uploadFolder"), disabled: phase !== "ready", onClick: () => void pickUpload("folder") },
            ],
          }}
        >
          <Button size="small" aria-label={t("zdx.label.ssh.upload")} icon={<UploadOutlined />} disabled={phase !== "ready"} />
        </Dropdown>
        <Select
          size="small"
          aria-label={t("zdx.label.ssh.theme")}
          className="w-28"
          popupMatchSelectWidth={false}
          value={themeId}
          onChange={(value) => setThemeId(value as SshThemeId)}
          options={sshThemes.map((theme) => ({ value: theme, label: t(`zdx.label.ssh.theme${theme[0].toUpperCase()}${theme.slice(1)}`) }))}
        />
        <Tooltip title={t("zdx.label.ssh.appearance")}>
          <Button size="small" aria-label={t("zdx.label.ssh.appearance")} icon={<FontColorsOutlined />} onClick={() => setAppearanceOpen(true)} />
        </Tooltip>
        <Input
          ref={searchInputRef}
          size="small"
          allowClear
          className="ml-auto w-40"
          aria-label={t("zdx.label.ssh.searchOutput")}
          placeholder={t("zdx.placeholder.ssh.searchOutput")}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            if (event.target.value) searchRef.current?.findNext(event.target.value, { incremental: true });
          }}
          onPressEnter={() => {
            if (query) searchRef.current?.findNext(query);
          }}
        />
        <Tooltip title={t("zdx.label.ssh.screenReader")}>
          <Switch size="small" aria-label={t("zdx.label.ssh.screenReader")} checked={reader} onChange={(checked) => {
            setReader(checked);
            if (termRef.current) termRef.current.options.screenReaderMode = checked;
          }} />
        </Tooltip>
        <Tooltip title={t("zdx.text.ssh.leaveHint")}>
          <Button size="small" aria-label={t("zdx.label.ssh.leaveTerminal")} onClick={() => termRef.current?.blur()}>{t("zdx.label.ssh.leaveTerminal")}</Button>
        </Tooltip>
        <Tooltip title={t("zdx.label.ssh.reconnect")}>
          <Button size="small" aria-label={t("zdx.label.ssh.reconnect")} icon={<ReloadOutlined />} onClick={onReconnect} />
        </Tooltip>
        <Tooltip title={t("zdx.label.ssh.disconnect")}>
          <Button size="small" aria-label={t("zdx.label.ssh.disconnect")} icon={<DisconnectOutlined />} onClick={() => {
            if (sessionRef.current) void sshDisconnect(sessionRef.current);
          }} />
        </Tooltip>
      </div>
      {notice && <Alert className="mx-3" type="warning" showIcon closable message={notice} onClose={() => setNotice("")} />}
      <div className="relative min-h-0 flex-1" style={{ background: palette.background }}>
        <div ref={containerRef} className="h-full w-full overflow-hidden" aria-label={t("zdx.label.ssh.terminal", { name: host.name })} />
        {menuAt && (
          <Dropdown
            open
            trigger={["click"]}
            getPopupContainer={() => document.body}
            onOpenChange={(open) => { if (!open) setMenuAt(null); }}
            menu={{
              items: [
                { key: "copy", label: t("zdx.label.ssh.copy"), disabled: !termRef.current?.getSelection() },
                { key: "paste", label: t("zdx.label.ssh.paste") },
                { key: "selectAll", label: t("zdx.label.ssh.selectAll") },
                { key: "clear", label: t("zdx.label.ssh.clearTerminal") },
                { type: "divider" },
                { key: "search", label: t("zdx.label.ssh.searchOutput") },
              ],
              onClick: ({ key }) => {
                const term = termRef.current;
                setMenuAt(null);
                if (!term) return;
                if (key === "copy") void copySelection(term);
                if (key === "paste") void navigator.clipboard.readText().then((text) => paste(term, text, t)).catch(() => undefined);
                if (key === "selectAll") term.selectAll();
                if (key === "clear") term.clear();
                if (key === "search") searchInputRef.current?.focus();
              },
            }}
          >
            <span className="fixed z-30 h-px w-px" style={{ left: menuAt.x, top: menuAt.y }} />
          </Dropdown>
        )}
        {dragOver && (
          <div className="pointer-events-none absolute inset-3 flex items-center justify-center border border-dashed" style={{ borderColor: palette.foreground }}>
            <span className="px-3 py-1 text-sm" style={{ color: palette.foreground, background: palette.background }}>
              {cwd ? t("zdx.text.ssh.dropHint", { path: cwd }) : t("zdx.text.ssh.dropPending")}
            </span>
          </div>
        )}
        {closed && (
          <div className="absolute inset-x-6 top-4">
            <Alert
              type="warning"
              showIcon
              message={closed}
              action={<Button size="small" onClick={onReconnect}>{t("zdx.label.ssh.reconnect")}</Button>}
            />
          </div>
        )}
      </div>
      {tasks.length > 0 && (
        <div className="max-h-28 shrink-0 overflow-auto px-3 py-2">
          {tasks.map((task) => {
            const percent = transferPercent(task.done, task.total);
            const text = transferText(t, i18n.language, task);
            return (
              <div key={task.taskId} className="py-1">
                <div className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-xs">{text}</span>
                  {task.status === "active" ? (
                    <Button size="small" aria-label={t("zdx.label.ssh.cancelUpload")} onClick={() => void sshTransferCancel(task.taskId)}>{t("zdx.label.ssh.cancel")}</Button>
                  ) : transferClosesItself(task) ? null : (
                    <Button size="small" aria-label={t("zdx.label.ssh.dismissTransfer")} onClick={() => setTasks((current) => current.filter((item) => item.taskId !== task.taskId))}>{t("zdx.label.ssh.dismissTransfer")}</Button>
                  )}
                </div>
                {task.status === "active" && (
                  <div className="mt-1 h-1.5 w-full" style={{ background: "rgba(127,127,127,0.28)" }} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-label={text}>
                    <div className="h-full" style={{ width: `${percent}%`, background: palette.foreground }} />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
      <Modal
        open={Boolean(conflict)}
        title={t("zdx.text.ssh.conflictTitle")}
        onCancel={() => {
          if (!conflict) return;
          const taskId = conflict.taskId;
          setConflicts((current) => current.filter((item) => item.taskId !== taskId));
          void sshTransferCancel(taskId);
        }}
        footer={[
          <Button key="skip" onClick={() => void chooseConflict(conflict, "skip", applyAll, setConflicts)}>{t("zdx.label.ssh.skipUpload")}</Button>,
          <Button key="overwrite" onClick={() => void chooseConflict(conflict, "overwrite", applyAll, setConflicts)}>{t("zdx.label.ssh.overwrite")}</Button>,
          <Button key="rename" type="primary" onClick={() => void chooseConflict(conflict, "rename", applyAll, setConflicts)}>{t("zdx.label.ssh.renameUpload")}</Button>,
        ]}
      >
        <p>{t("zdx.text.ssh.conflictBody", { name: conflict?.name ?? "" })}</p>
        <Checkbox checked={applyAll} onChange={(event) => setApplyAll(event.target.checked)}>{t("zdx.label.ssh.conflictApplyAll")}</Checkbox>
      </Modal>
      <TerminalAppearance open={appearanceOpen} onClose={() => setAppearanceOpen(false)} />
    </div>
  );
}

function transferText(t: (key: string, options?: Record<string, unknown>) => string, language: string, task: TransferRow) {
  if (task.status === "failed") return sshErrorText(t, { code: task.code || "transfer_failed", params: {} });
  if (task.status === "done") {
    if (task.uploaded > 0) return t("zdx.text.ssh.transferDone", { count: task.uploaded, path: task.target });
    return t("zdx.text.ssh.transferNone");
  }
  if (task.name) return t("zdx.text.ssh.transferProgress", { name: task.name, percent: transferPercent(task.done, task.total), done: formatBytes(task.done, language), total: formatBytes(task.total, language) });
  if (task.target) return t("zdx.text.ssh.uploadTarget", { path: task.target });
  return t("zdx.text.ssh.uploadTargetPending");
}

function chooseConflict(
  conflict: { taskId: number; name: string } | undefined,
  policy: "overwrite" | "rename" | "skip",
  applyAll: boolean,
  setConflicts: (update: (current: { taskId: number; name: string }[]) => { taskId: number; name: string }[]) => void,
) {
  if (!conflict) return;
  setConflicts((current) => current.filter((item) => item.taskId !== conflict.taskId || item.name !== conflict.name));
  return sshTransferDecide(conflict.taskId, policy, applyAll);
}

async function copySelection(term: Terminal) {
  const selection = term.getSelection();
  if (!selection) return;
  if (useSshAppearanceStore.getState().copySkipsHistory && isDesktop()) {
    await clipboardIgnoreNext().catch(() => undefined);
  }
  await navigator.clipboard.writeText(selection);
}

function paste(term: Terminal, text: string, t: (key: string) => string) {
  if (!text) return;
  const multiline = text.includes("\n") || text.includes("\r");
  if (multiline && useSshAppearanceStore.getState().confirmPaste) {
    Modal.confirm({
      title: t("zdx.text.ssh.pasteConfirmTitle"),
      content: t("zdx.text.ssh.pasteConfirmBody"),
      okText: t("zdx.label.ssh.confirm"),
      cancelText: t("zdx.label.ssh.cancel"),
      onOk: () => term.paste(text),
    });
    return;
  }
  term.paste(text);
}
