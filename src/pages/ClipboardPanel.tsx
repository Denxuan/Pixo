import { getCurrentWindow } from "@tauri-apps/api/window";

import { ClipboardRequestError, clipboardSetLabel, dismissClipboardPanel } from "@/api/clipboard";
import { ScreenshotRequestError, screenshotPinClipboard } from "@/api/screenshot";
import { screenshotErrorText } from "@/utils/screenshotMessage";
import HistoryList from "@/components/clipboard/HistoryList";
import Kbd from "@/components/Kbd";
import { useClipboardHistory } from "@/hooks/useClipboard";
import { CLIP_FILTERS, isClipFilter } from "@/types/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";
import { DEFAULT_DELETE_SHORTCUT, DEFAULT_PIN_SHORTCUT, eventMatchesShortcut, formatShortcut, isApplePlatform } from "@/utils/shortcut";

function focusSearch() {
  const active = document.activeElement;
  if (active instanceof HTMLElement && active.id === "clipboard-pin-title") return;
  const field = document.getElementById("clipboard-panel-search");
  const input = field instanceof HTMLInputElement ? field : field?.querySelector("input");
  if (!(input instanceof HTMLInputElement) || active === input) return;
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
  const [stickingId, setStickingId] = useState<number | null>(null);
  const [messageTone, setMessageTone] = useState<"success" | "warning">("success");
  const [clearOpen, setClearOpen] = useState(false);
  const [clearAll, setClearAll] = useState(false);
  const [labelDraft, setLabelDraft] = useState("");
  const copyTimer = useRef(0);
  const copySeq = useRef(0);
  const selectedId = useRef<number | null>(null);
  const pendingClose = useRef<number | null>(null);
  const titleFocus = useRef<(() => void) | null>(null);
  const labelFocused = useRef(false);
  const focusTitle = useRef(false);
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
  };
  const panelHelpKey = pasteAutomatically ? "zdx.text.clipboard.panelHelpPaste" : copyOnSelect ? "zdx.text.clipboard.panelHelpSelect" : "zdx.text.clipboard.panelHelp";
  const panelHelp = t(panelHelpKey, { ...shortcutHelp, plain: hintKeys.plain });
  const numberShortcuts = Array.from({ length: 9 }, (_, position) => {
    const number = String(position + 1);
    return {
      copy: formatShortcut(`CommandOrControl+${number}`, macos),
      paste: formatShortcut(`Alt+${number}`, macos),
    };
  });
  const active = history.items[index];

  useEffect(() => {
    void history.loadSettings();
    return () => window.clearTimeout(copyTimer.current);
  }, []);

  useEffect(() => {
    const nodes = [document.documentElement, document.body, document.getElementById("root")].filter((node): node is HTMLElement => node instanceof HTMLElement);
    const previous = nodes.map((node) => node.style.background);
    for (const node of nodes) node.style.background = "transparent";
    return () => {
      nodes.forEach((node, position) => {
        node.style.background = previous[position] ?? "";
      });
    };
  }, []);

  useEffect(() => {
    const onShown = () => {
      history.clearQuery();
      history.setFilter("all");
      selectedId.current = null;
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
      if (labelFocused.current || focusTitle.current) return;
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
    if (!isDesktop()) return;
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
    const id = selectedId.current;
    if (id != null) {
      const next = history.items.findIndex((item) => item.id === id);
      if (next >= 0) {
        if (next !== index) setIndex(next);
        return;
      }
    }
    const clamped = Math.min(index, Math.max(history.items.length - 1, 0));
    if (clamped !== index) setIndex(clamped);
    selectedId.current = history.items[clamped]?.id ?? null;
  }, [history.items, index]);

  useEffect(() => {
    if (!active) return;
    document.getElementById(`clip-${active.id}`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  useEffect(() => {
    if (labelFocused.current) return;
    setLabelDraft(active?.label ?? "");
  }, [active?.id, active?.label]);

  useEffect(() => {
    if (!active?.pinned || !focusTitle.current) return;
    focusTitle.current = false;
    const focus = titleFocus.current;
    const frame = window.requestAnimationFrame(() => focus?.());
    return () => window.cancelAnimationFrame(frame);
  }, [active?.id, active?.pinned]);

  async function useItem(id: number, mode: "copy" | "paste" | "plain", dismiss: boolean) {
    const seq = ++copySeq.current;
    try {
      history.suppressChanged();
      const result = await history.copyItem(id, { plain: mode === "plain", paste: mode !== "copy", reorder: dismiss });
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
    if (next >= 0) {
      selectedId.current = id;
      setIndex(next);
    }
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
    selectedId.current = item.id;
    setIndex(next);
    if (copyOnSelect) queueCopy(item.id);
  }

  function askClear(all: boolean) {
    window.clearTimeout(copyTimer.current);
    setClearAll(all);
    setClearOpen(true);
  }

  async function saveLabel() {
    if (!active?.pinned) return;
    const next = labelDraft.trim();
    if (next === (active.label ?? "")) return;
    try {
      await clipboardSetLabel(active.id, next);
      await history.reload();
    } catch (error) {
      if (error instanceof ClipboardRequestError) {
        setMessageTone("warning");
        setMessage(clipboardErrorText(t, error));
      }
    }
  }

  async function pinActive(pinned: boolean) {
    if (!active) return;
    if (pinned) focusTitle.current = true;
    await history.pinItem(active.id, pinned);
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (clearOpen || event.nativeEvent.isComposing) return;
    const field = event.target instanceof Element ? event.target.closest("input, textarea") : null;
    if (field instanceof HTMLInputElement && field.id === "clipboard-pin-title") {
      if (event.key === "Escape") {
        event.preventDefault();
        field.blur();
      }
      return;
    }
    if (event.key !== "Escape" && event.target instanceof Element && event.target.closest(".ant-select")) return;
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
      if (event.target instanceof Element && event.target.closest("button")) return;
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
      if (active) void pinActive(!active.pinned);
    } else if (eventMatchesShortcut(event, deleteShortcut)) {
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      if (active) void history.deleteItem(active.id);
    }
  }

  const keyHints = pasteAutomatically
      ? [
          { keys: formatShortcut("Enter", macos), label: t("zdx.label.clipboard.paste") },
          { keys: hintKeys.paste, label: t("zdx.label.clipboard.copy") },
          { keys: hintKeys.plain, label: t("zdx.label.clipboard.pastePlain") },
        ]
      : [
          { keys: formatShortcut("Enter", macos), label: t("zdx.label.clipboard.copy") },
          { keys: hintKeys.paste, label: t("zdx.label.clipboard.paste") },
          { keys: hintKeys.plain, label: t("zdx.label.clipboard.pastePlain") },
        ];
  const helpContent = (
    <Flex vertical gap={6} style={{ maxWidth: 320 }}>
      <span>{panelHelp}</span>
      {macos && <span>{t("zdx.text.clipboard.hintPasteAccess")}</span>}
    </Flex>
  );

  return (
    <div className="box-border h-full" style={{ padding: "8px 12px 12px 8px", background: "transparent" }}>
    <Flex
      vertical
      className="h-full overflow-hidden"
      onKeyDownCapture={onKeyDown}
      aria-label={t("zdx.label.clipboard.title")}
      style={{
        background: token.colorBgContainer,
        color: token.colorText,
        border: `1px solid ${token.colorBorder}`,
        borderRadius: token.borderRadiusLG,
        boxShadow: "0 10px 28px rgba(20, 16, 12, 0.18)",
      }}
    >
      <div data-tauri-drag-region className="flex shrink-0 items-center gap-2 px-3 pb-3 pt-3" style={{ borderBottom: `1px solid ${token.colorBorder}` }}>
        <Input
          id="clipboard-panel-search"
          allowClear
          prefix={<SearchOutlined style={{ color: token.colorTextTertiary }} />}
          value={history.query}
          onChange={(event) => history.setQuery(event.target.value)}
          placeholder={t("zdx.placeholder.clipboard.search")}
          aria-label={t("zdx.label.clipboard.search")}
          className="min-w-0 flex-1"
        />
        <Select
          value={history.filter}
          aria-label={t("zdx.label.clipboard.filterKind")}
          popupMatchSelectWidth={false}
          onChange={(value) => {
            if (isClipFilter(value)) history.setFilter(value);
          }}
          options={CLIP_FILTERS.map((item) => ({ value: item.id, label: t(item.label) }))}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-3 py-3">
        {history.items.length === 0 && <Empty className="py-10" image={Empty.PRESENTED_IMAGE_SIMPLE} description={history.query ? t("zdx.text.clipboard.noResults") : t("zdx.text.clipboard.empty")} />}
        {history.items.length > 0 && (
          <HistoryList
            items={history.items}
            activeId={active?.id ?? null}
            onActive={choose}
            onCopy={(id) => {
              window.clearTimeout(copyTimer.current);
              void useItem(id, "copy", true);
            }}
            onPin={(id, pinned) => {
              if (pinned && active?.id === id) focusTitle.current = true;
              void history.pinItem(id, pinned);
            }}
            onStick={(id) => {
              setStickingId(id);
              setMessage("");
              void screenshotPinClipboard(id).then(() => {
                setMessageTone("success");
                setMessage(t("zdx.text.clipboard.stuck"));
              }).catch((error: unknown) => {
                setMessageTone("warning");
                setMessage(error instanceof ScreenshotRequestError ? screenshotErrorText(t, error) : t("zdx.text.screenshot.error.unknown"));
              }).finally(() => setStickingId(null));
            }}
            stickingId={stickingId}
            onDelete={(id) => void history.deleteItem(id)}
            numberShortcuts={numberShortcuts}
          />
        )}
      </div>
      {message && <Alert type={messageTone} showIcon message={message} style={{ margin: "0 8px 8px" }} />}
      {active?.pinned && (
        <div className="flex shrink-0 flex-col gap-2 px-3 pb-2">
          <Input
            id="clipboard-pin-title"
            size="small"
            aria-label={t("zdx.label.clipboard.pinTitle")}
            placeholder={t("zdx.placeholder.clipboard.pinTitle")}
            value={labelDraft}
            ref={(node) => {
              titleFocus.current = () => node?.focus();
            }}
            onFocus={() => { labelFocused.current = true; }}
            onBlur={() => {
              labelFocused.current = false;
              void saveLabel();
            }}
            onPressEnter={() => void saveLabel()}
          />
        </div>
      )}
      <div data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-2 pl-3 pr-1.5" style={{ borderTop: `1px solid ${token.colorBorder}`, background: token.colorFillQuaternary }}>
        <div data-tauri-drag-region aria-label={t("zdx.label.clipboard.shortcutHints")} className="flex min-w-0 flex-1 items-center gap-3 overflow-hidden">
          {keyHints.map((hint) => (
            <span key={hint.label} className="flex shrink-0 items-center gap-1.5 whitespace-nowrap" style={{ fontSize: token.fontSizeSM, color: token.colorTextSecondary }}>
              <Kbd>{hint.keys}</Kbd>
              {hint.label}
            </span>
          ))}
        </div>
        <Flex align="center" className="shrink-0">
          <Tooltip
            title={helpContent}
            placement="topRight"
            styles={{
              root: { maxWidth: 320 },
              container: { maxWidth: 320, maxHeight: "min(240px, calc(100vh - 80px))", overflow: "auto" },
            }}
          >
            <Button type="text" size="small" aria-label={t("zdx.label.clipboard.help")} aria-description={panelHelp} icon={<InfoCircleOutlined />} style={{ color: token.colorTextSecondary }} />
          </Tooltip>
          <Popconfirm
            title={t("zdx.label.clipboard.clear")}
            description={t(clearAll ? "zdx.text.clipboard.clearAllConfirm" : "zdx.text.clipboard.clearConfirm")}
            okText={t("zdx.label.clipboard.confirm")}
            cancelText={t("zdx.label.clipboard.cancel")}
            okButtonProps={{ danger: true }}
            placement="topRight"
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
            <Button type="text" size="small" danger aria-label={t("zdx.label.clipboard.clear")} icon={<ClearOutlined />} onMouseDown={(event) => setClearAll(event.altKey && event.shiftKey)} />
          </Popconfirm>
          <Button type="text" size="small" aria-label={t("zdx.label.clipboard.close")} icon={<CloseOutlined />} style={{ color: token.colorTextSecondary }} onClick={() => dismissClipboardPanel()} />
        </Flex>
      </div>
    </Flex>
    </div>
  );
}
