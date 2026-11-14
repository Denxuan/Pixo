import { getCurrentWindow } from "@tauri-apps/api/window";

import { ClipboardRequestError, dismissClipboardPanel } from "@/api/clipboard";
import HistoryList from "@/components/clipboard/HistoryList";
import { useClipboardHistory } from "@/hooks/useClipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { DEFAULT_DELETE_SHORTCUT, DEFAULT_PIN_SHORTCUT, eventMatchesShortcut, formatShortcut, isApplePlatform } from "@/utils/shortcut";

function focusSearch() {
  const field = document.getElementById("clipboard-panel-search");
  const input = field instanceof HTMLInputElement ? field : field?.querySelector("input");
  if (!(input instanceof HTMLInputElement)) return;
  input.focus({ preventScroll: true });
  const end = input.value.length;
  input.setSelectionRange(end, end);
}

export default function ClipboardPanel() {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const history = useClipboardHistory();
  const [index, setIndex] = useState(0);
  const [message, setMessage] = useState("");
  const [messageTone, setMessageTone] = useState<"success" | "warning">("success");
  const [clearOpen, setClearOpen] = useState(false);
  const [clearAll, setClearAll] = useState(false);
  const copyTimer = useRef(0);
  const copySeq = useRef(0);
  const pendingClose = useRef<number | null>(null);
  const copyOnSelect = history.settings?.copyOnSelect !== false;
  const pasteAutomatically = history.settings?.pasteAutomatically !== false;
  const macos = isApplePlatform();
  const pinShortcut = history.settings?.pinShortcut || DEFAULT_PIN_SHORTCUT;
  const deleteShortcut = history.settings?.deleteShortcut || DEFAULT_DELETE_SHORTCUT;
  const shortcutHelp = {
    pin: formatShortcut(pinShortcut, macos),
    remove: formatShortcut(deleteShortcut, macos),
    option: macos ? "⌥" : "Alt",
    command: macos ? "⌘" : "Ctrl",
  };
  const hintKeys = {
    paste: formatShortcut("Alt+Enter", macos),
    plain: formatShortcut("Alt+Shift+Enter", macos),
    option: shortcutHelp.option,
    numberCopy: macos ? "⌘1–9" : "Ctrl+1–9",
    numberPaste: macos ? "⌥1–9" : "Alt+1–9",
  };
  const panelHelpKey = pasteAutomatically ? "zdx.text.clipboard.panelHelpPaste" : copyOnSelect ? "zdx.text.clipboard.panelHelpSelect" : "zdx.text.clipboard.panelHelp";
  const panelHelp = t(panelHelpKey, { ...shortcutHelp, plain: hintKeys.plain });
  const numberShortcuts = Array.from({ length: 9 }, (_, index) => {
    const number = String(index + 1);
    return {
      copy: formatShortcut(`CommandOrControl+${number}`, macos),
      paste: formatShortcut(`Alt+${number}`, macos),
    };
  });

  useEffect(() => {
    void history.loadSettings();
    return () => window.clearTimeout(copyTimer.current);
  }, []);

  useEffect(() => {
    const nodes = [document.documentElement, document.body, document.getElementById("root")].filter((node): node is HTMLElement => node instanceof HTMLElement);
    const previous = nodes.map((node) => node.style.background);
    for (const node of nodes) node.style.background = "transparent";
    return () => {
      nodes.forEach((node, index) => {
        node.style.background = previous[index] ?? "";
      });
    };
  }, []);

  useEffect(() => {
    const onShown = () => {
      history.clearQuery();
      setIndex(0);
      setMessage("");
      setMessageTone("success");
      window.requestAnimationFrame(focusSearch);
      window.setTimeout(focusSearch, 30);
    };
    window.addEventListener("clipboard-panel-shown", onShown);
    return () => window.removeEventListener("clipboard-panel-shown", onShown);
  }, [history.clearQuery]);

  useEffect(() => {
    const frame = window.requestAnimationFrame(focusSearch);
    const later = window.setTimeout(focusSearch, 50);
    return () => {
      window.cancelAnimationFrame(frame);
      window.clearTimeout(later);
    };
  }, []);

  useEffect(() => {
    let hideTimer = 0;
    let armTimer = 0;
    let armed = false;
    const onFocus = () => {
      window.clearTimeout(hideTimer);
      window.clearTimeout(armTimer);
      armed = false;
      focusSearch();
      window.setTimeout(focusSearch, 30);
      armTimer = window.setTimeout(() => {
        armed = document.hasFocus();
      }, 600);
    };
    const onBlur = () => {
      window.clearTimeout(hideTimer);
      window.clearTimeout(armTimer);
      if (!armed) return;
      armed = false;
      hideTimer = window.setTimeout(() => {
        if (document.hasFocus()) return;
        dismissClipboardPanel();
      }, 200);
    };
    const unlisten = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) onFocus();
    });
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    if (document.hasFocus()) onFocus();
    return () => {
      void unlisten.then((stop) => stop());
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      window.clearTimeout(hideTimer);
      window.clearTimeout(armTimer);
    };
  }, []);

  useEffect(() => {
    if (index > history.items.length - 1) setIndex(Math.max(history.items.length - 1, 0));
  }, [history.items.length, index]);

  const active = history.items[index];

  useEffect(() => {
    if (!active) return;
    document.getElementById(`clip-${active.id}`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  async function useItem(id: number, mode: "copy" | "paste" | "plain", dismiss: boolean) {
    const seq = ++copySeq.current;
    try {
      history.suppressChanged();
      const result = await history.copyItem(id, { plain: mode === "plain", paste: mode !== "copy" });
      if (seq !== copySeq.current) return;
      if (result.missing.length > 0) {
        pendingClose.current = null;
        setMessageTone("warning");
        setMessage(t("zdx.text.clipboard.fileMissing", { count: result.missing.length }));
        return;
      }
      if (result.pasteError) {
        pendingClose.current = null;
        setMessageTone("warning");
        setMessage(t(result.pasteError === "permission_denied" ? "zdx.text.clipboard.pasteNeedsAccess" : "zdx.text.clipboard.pasteUnsupported"));
        return;
      }
      if (dismiss) dismissClipboardPanel();
    } catch (error) {
      if (seq !== copySeq.current) return;
      pendingClose.current = null;
      setMessageTone("warning");
      if (error instanceof ClipboardRequestError) setMessage(clipboardErrorText(t, error));
    }
  }

  function queueCopy(id: number) {
    window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => void useItem(id, "copy", false), 120);
  }

  function choose(id: number, pointer?: { altKey: boolean; shiftKey: boolean }) {
    const next = history.items.findIndex((item) => item.id === id);
    if (next >= 0) setIndex(next);
    const mode = selectedMode(pointer);
    if (mode === "copy" && (!copyOnSelect || pendingClose.current === id)) return;
    pendingClose.current = id;
    window.clearTimeout(copyTimer.current);
    void useItem(id, mode, true);
  }

  function selectedMode(pointer?: { altKey: boolean; shiftKey: boolean }) {
    if (pointer?.altKey && pointer.shiftKey) return "plain" as const;
    if (pointer?.altKey) return pasteAutomatically ? "copy" as const : "paste" as const;
    return pasteAutomatically ? "paste" as const : "copy" as const;
  }

  function move(delta: number) {
    const next = Math.min(Math.max(index + delta, 0), Math.max(history.items.length - 1, 0));
    const item = history.items[next];
    if (!item || next === index) return;
    setIndex(next);
    if (copyOnSelect) queueCopy(item.id);
  }

  function askClear(all: boolean) {
    window.clearTimeout(copyTimer.current);
    setClearAll(all);
    setClearOpen(true);
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (clearOpen || event.nativeEvent.isComposing) return;
    const digit = /^Digit([1-9])$/.exec(event.code) ?? /^Numpad([1-9])$/.exec(event.code);
    const position = digit ? Number(digit[1]) - 1 : null;
    if (event.key === "Escape") {
      event.preventDefault();
      dismissClipboardPanel();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Backspace" && event.altKey && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      askClear(event.shiftKey);
    } else if (position !== null && (event.metaKey || event.ctrlKey) && !event.altKey) {
      const item = history.items[position];
      if (!item) return;
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      void useItem(item.id, "copy", true);
    } else if (position !== null && event.altKey && !event.metaKey && !event.ctrlKey) {
      const item = history.items[position];
      if (!item) return;
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      void useItem(item.id, event.shiftKey ? "plain" : "paste", true);
    } else if (event.key === "Enter") {
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      if (active) void useItem(active.id, selectedMode({ altKey: event.altKey, shiftKey: event.shiftKey }), true);
    } else if (!event.metaKey && !event.ctrlKey && !event.altKey && !event.repeat && history.query.length === 0 && event.key.length === 1) {
      const item = history.items.find((entry) => entry.pinKey === event.key.toLowerCase());
      if (!item) return;
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      void useItem(item.id, pasteAutomatically ? "paste" : "copy", true);
    } else if (eventMatchesShortcut(event, pinShortcut)) {
      event.preventDefault();
      if (active) void history.pinItem(active.id, !active.pinned);
    } else if (eventMatchesShortcut(event, deleteShortcut)) {
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      if (active) void history.deleteItem(active.id);
    }
  }

  return (
    <Flex
      vertical
      className="h-screen overflow-hidden"
      onKeyDownCapture={onKeyDown}
      style={{
        background: token.colorBgElevated,
        color: token.colorText,
        borderRadius: 20,
        border: `1px solid ${token.colorBorderSecondary}`,
      }}
    >
      <Flex justify="space-between" align="center" data-tauri-drag-region style={{ padding: "10px 12px", borderBottom: `1px solid ${token.colorBorderSecondary}` }}>
        <Flex align="center" gap={4} data-tauri-drag-region className="min-w-0">
          <Typography.Title level={5} data-tauri-drag-region style={{ margin: 0 }}>{t("zdx.label.clipboard.title")}</Typography.Title>
          <Tooltip title={panelHelp}>
            <Button type="text" size="small" aria-label={panelHelp} icon={<InfoCircleOutlined />} />
          </Tooltip>
        </Flex>
        <Flex align="center">
          <Popconfirm
            title={t("zdx.label.clipboard.clear")}
            description={t(clearAll ? "zdx.text.clipboard.clearAllConfirm" : "zdx.text.clipboard.clearConfirm")}
            okText={t("zdx.label.clipboard.confirm")}
            cancelText={t("zdx.label.clipboard.cancel")}
            okButtonProps={{ danger: true }}
            placement="bottomRight"
            open={clearOpen}
            onOpenChange={(open) => {
              if (open) window.clearTimeout(copyTimer.current);
              setClearOpen(open);
            }}
            onConfirm={() => {
              setClearOpen(false);
              return history.clearHistory(!clearAll).then(() => {
                setMessageTone("success");
                setMessage(t(clearAll ? "zdx.text.clipboard.clearedAll" : "zdx.text.clipboard.cleared"));
              }).catch((error) => {
                if (error instanceof ClipboardRequestError) setMessage(clipboardErrorText(t, error));
                throw error;
              });
            }}
          >
            <Button type="text" danger aria-label={t("zdx.label.clipboard.clear")} icon={<ClearOutlined />} onMouseDown={(event) => setClearAll(event.altKey && event.shiftKey)} />
          </Popconfirm>
          <Button type="text" aria-label={t("zdx.label.clipboard.close")} icon={<CloseOutlined />} onClick={() => dismissClipboardPanel()} />
        </Flex>
      </Flex>
      <Flex vertical gap={8} style={{ padding: "10px 12px 8px" }}>
        <Input
          id="clipboard-panel-search"
          allowClear
          prefix={<SearchOutlined />}
          value={history.query}
          onChange={(event) => history.setQuery(event.target.value)}
          placeholder={t("zdx.placeholder.clipboard.search")}
          aria-label={t("zdx.label.clipboard.search")}
        />
      </Flex>
      <div className="min-h-0 flex-1 overflow-auto" style={{ padding: "0 12px 12px" }}>
        {history.items.length === 0 && <Empty description={history.query ? t("zdx.text.clipboard.noResults") : t("zdx.text.clipboard.empty")} />}
        {history.items.length > 0 && (
          <HistoryList
            items={history.items}
            activeId={active?.id ?? null}
            onActive={choose}
            onCopy={(id) => {
              window.clearTimeout(copyTimer.current);
              void useItem(id, "copy", true);
            }}
            onPin={(id, pinned) => void history.pinItem(id, pinned)}
            onDelete={(id) => void history.deleteItem(id)}
            numberShortcuts={numberShortcuts}
          />
        )}
      </div>
      <Flex vertical gap={2} className="shrink-0" aria-label={t("zdx.label.clipboard.shortcutHints")} style={{ padding: "8px 12px 10px", borderTop: `1px solid ${token.colorBorderSecondary}` }}>
        <Typography.Text type="secondary" style={{ fontSize: token.fontSizeSM }}>{t(pasteAutomatically ? "zdx.text.clipboard.hintPasteAuto" : "zdx.text.clipboard.hintPaste", hintKeys)}</Typography.Text>
        <Typography.Text type="secondary" style={{ fontSize: token.fontSizeSM }}>{t("zdx.text.clipboard.hintPlain", hintKeys)}</Typography.Text>
        <Typography.Text type="secondary" style={{ fontSize: token.fontSizeSM }}>{t("zdx.text.clipboard.hintNumbers", hintKeys)}</Typography.Text>
        {macos && <Typography.Text type="secondary" style={{ fontSize: token.fontSizeSM }}>{t("zdx.text.clipboard.hintPasteAccess")}</Typography.Text>}
      </Flex>
      {message && <Alert type={messageTone} showIcon message={message} style={{ margin: "0 12px 12px" }} />}
    </Flex>
  );
}
