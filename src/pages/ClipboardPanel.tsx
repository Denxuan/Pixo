import { getCurrentWindow } from "@tauri-apps/api/window";

import { ClipboardRequestError, dismissClipboardPanel } from "@/api/clipboard";
import HistoryList from "@/components/clipboard/HistoryList";
import { useClipboardHistory } from "@/hooks/useClipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";

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
  const copyTimer = useRef(0);
  const copySeq = useRef(0);
  const copyOnSelect = history.settings?.copyOnSelect !== false;

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

  async function copyById(id: number, dismiss: boolean) {
    const seq = ++copySeq.current;
    try {
      history.suppressChanged();
      const result = await history.copyItem(id);
      if (seq !== copySeq.current) return;
      if (result.missing.length > 0) {
        setMessage(t("zdx.text.clipboard.fileMissing", { count: result.missing.length }));
      } else if (dismiss) {
        setMessage(t("zdx.text.clipboard.panelHint"));
      }
      if (dismiss) {
        window.setTimeout(() => {
          if (seq === copySeq.current) dismissClipboardPanel();
        }, 600);
      }
    } catch (error) {
      if (seq !== copySeq.current) return;
      if (error instanceof ClipboardRequestError) setMessage(clipboardErrorText(t, error));
    }
  }

  function queueCopy(id: number) {
    window.clearTimeout(copyTimer.current);
    copyTimer.current = window.setTimeout(() => void copyById(id, false), 120);
  }

  function choose(id: number) {
    const next = history.items.findIndex((item) => item.id === id);
    if (next >= 0) setIndex(next);
    if (copyOnSelect) queueCopy(id);
  }

  function move(delta: number) {
    const next = Math.min(Math.max(index + delta, 0), Math.max(history.items.length - 1, 0));
    const item = history.items[next];
    if (!item || next === index) return;
    setIndex(next);
    if (copyOnSelect) queueCopy(item.id);
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      dismissClipboardPanel();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      window.clearTimeout(copyTimer.current);
      if (active) void copyById(active.id, true);
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
      event.preventDefault();
      if (active) void history.pinItem(active.id, !active.pinned);
    }
  }

  return (
    <Flex
      vertical
      className="h-screen overflow-hidden"
      onKeyDown={onKeyDown}
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
          <Tooltip title={t(copyOnSelect ? "zdx.text.clipboard.panelHelpSelect" : "zdx.text.clipboard.panelHelp")}>
            <Button type="text" size="small" aria-label={t(copyOnSelect ? "zdx.text.clipboard.panelHelpSelect" : "zdx.text.clipboard.panelHelp")} icon={<InfoCircleOutlined />} />
          </Tooltip>
        </Flex>
        <Button type="text" aria-label={t("zdx.label.clipboard.close")} icon={<CloseOutlined />} onClick={() => dismissClipboardPanel()} />
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
              void copyById(id, true);
            }}
            onPin={(id, pinned) => void history.pinItem(id, pinned)}
            onDelete={(id) => void history.deleteItem(id)}
          />
        )}
      </div>
      {message && <Alert type="success" showIcon message={message} style={{ margin: 16 }} />}
    </Flex>
  );
}
