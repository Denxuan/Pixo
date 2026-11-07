import { getCurrentWindow } from "@tauri-apps/api/window";

import { ClipboardRequestError } from "@/api/clipboard";
import HistoryList from "@/components/clipboard/HistoryList";
import { useClipboardHistory } from "@/hooks/useClipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";

export default function ClipboardPanel() {
  const { t } = useTranslation();
  const history = useClipboardHistory();
  const [index, setIndex] = useState(0);
  const [message, setMessage] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  useEffect(() => {
    let timer = 0;
    const onBlur = () => {
      timer = window.setTimeout(() => {
        void getCurrentWindow().hide();
      }, 300);
    };
    const onFocus = () => window.clearTimeout(timer);
    window.addEventListener("blur", onBlur);
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("focus", onFocus);
      window.clearTimeout(timer);
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

  async function copyById(id: number) {
    try {
      const result = await history.copyItem(id);
      setMessage(result.missing.length > 0 ? t("zdx.text.clipboard.fileMissing", { count: result.missing.length }) : t("zdx.text.clipboard.panelHint"));
      window.setTimeout(() => {
        void getCurrentWindow().hide();
      }, 600);
    } catch (error) {
      if (error instanceof ClipboardRequestError) setMessage(clipboardErrorText(t, error));
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void getCurrentWindow().hide();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      setIndex((current) => Math.min(current + 1, history.items.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setIndex((current) => Math.max(current - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (active) void copyById(active.id);
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "p") {
      event.preventDefault();
      if (active) void history.pinItem(active.id, !active.pinned);
    }
  }

  return (
    <div className="flex h-screen flex-col bg-[#f5f7fb] text-[#182340] dark:bg-[#111827] dark:text-[#e7ecf7]" onKeyDown={onKeyDown}>
      <div data-tauri-drag-region className="flex items-center justify-between px-4 pt-3">
        <h1 data-tauri-drag-region className="m-0 text-base font-semibold">{t("zdx.label.clipboard.title")}</h1>
        <button type="button" aria-label={t("zdx.label.clipboard.close")} onClick={() => void getCurrentWindow().hide()} className="rounded-lg px-2 py-1 text-sm text-[#405170] hover:bg-[#e8edf7] dark:text-[#d5def3] dark:hover:bg-[#243044]">{t("zdx.label.clipboard.close")}</button>
      </div>
      <div className="px-4 py-3">
        <input ref={searchRef} value={history.query} onChange={(event) => history.setQuery(event.target.value)} placeholder={t("zdx.placeholder.clipboard.search")} aria-label={t("zdx.label.clipboard.search")} className="w-full rounded-lg border border-[#d6deeb] bg-white px-3 py-2 text-sm outline-none focus:border-[#6073e9] dark:border-[#46546b] dark:bg-[#202b40]" />
        <p className="m-0 mt-2 text-xs text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.clipboard.panelHelp")}</p>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-4 pb-4">
        {history.items.length === 0 && <p className="text-sm text-[#667491] dark:text-[#aab7cf]">{history.query ? t("zdx.text.clipboard.noResults") : t("zdx.text.clipboard.empty")}</p>}
        <HistoryList items={history.items} activeId={active?.id ?? null} onActive={(id) => setIndex(history.items.findIndex((item) => item.id === id))} onCopy={(id) => void copyById(id)} onPin={(id, pinned) => void history.pinItem(id, pinned)} onDelete={(id) => void history.deleteItem(id)} />
      </div>
      {message && <p role="status" className="border-t border-[#dfe5f0] px-4 py-3 text-sm text-[#527a67] dark:border-[#34415a] dark:text-[#8ed9ad]">{message}</p>}
    </div>
  );
}
