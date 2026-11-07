import { ClipboardRequestError, clipboardClear, toClipboardError } from "@/api/clipboard";
import HistoryList from "@/components/clipboard/HistoryList";
import SettingsDialog from "@/components/clipboard/SettingsDialog";
import { useClipboardHistory } from "@/hooks/useClipboard";
import type { ClipFilter, ClipboardSettings } from "@/types/clipboard";
import { toSettingsUpdate } from "@/types/clipboard";
import { clipboardErrorText, clipboardReasonText, clipboardStatusText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";

const FILTERS: Array<{ id: ClipFilter; label: string }> = [
  { id: "all", label: "zdx.label.clipboard.filter.all" },
  { id: "text", label: "zdx.label.clipboard.filter.text" },
  { id: "image", label: "zdx.label.clipboard.filter.image" },
  { id: "files", label: "zdx.label.clipboard.filter.files" },
  { id: "pinned", label: "zdx.label.clipboard.filter.pinned" },
];

function filterClass(active: boolean) {
  if (active) return "rounded-full bg-[#344c91] px-3 py-1 text-sm text-white";
  return "rounded-full bg-[#e8edf7] px-3 py-1 text-sm text-[#31415f] dark:bg-[#243044] dark:text-[#d5def3]";
}

export default function ClipboardPage() {
  const { t } = useTranslation();
  const history = useClipboardHistory();
  const [activeId, setActiveId] = useState<number | null>(null);
  const [notice, setNotice] = useState("");
  const [enableOpen, setEnableOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const [keepPinned, setKeepPinned] = useState(true);
  const [pendingDelete, setPendingDelete] = useState<number | null>(null);
  const desktop = isDesktop();

  useEffect(() => {
    if (history.items.length === 0) {
      setActiveId(null);
      return;
    }
    if (!history.items.some((item) => item.id === activeId)) setActiveId(history.items[0].id);
  }, [history.items, activeId]);

  async function copy(id: number) {
    try {
      const result = await history.copyItem(id);
      setNotice(result.missing.length > 0 ? t("zdx.text.clipboard.fileMissing", { count: result.missing.length }) : t("zdx.text.clipboard.copied"));
    } catch (error) {
      if (error instanceof ClipboardRequestError) setNotice(clipboardErrorText(t, error));
    }
  }

  async function enable() {
    const current = history.settings ?? await history.loadSettings();
    await history.saveSettings(toSettingsUpdate({ ...current, enabled: true }));
    setEnableOpen(false);
  }

  async function openSettings() {
    await history.loadSettings();
    setSettingsOpen(true);
  }

  async function saveSettings(next: ClipboardSettings, confirm: boolean) {
    await history.saveSettings(toSettingsUpdate(next, confirm));
    await history.reload();
  }

  const status = history.status;
  const reason = clipboardReasonText(t, status?.reason ?? null);

  return (
    <section className="mt-5 flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex shrink-0 flex-wrap items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-[#e6ebff] text-xl text-[#4c5fda]"><HistoryOutlined aria-hidden="true" /></div>
          <div>
            <h1 className="m-0 text-3xl font-semibold tracking-tight">{t("zdx.label.clipboard.title")}</h1>
            <p className="m-0 mt-1 text-sm text-[#687895] dark:text-[#aab7cf]">{t("zdx.text.clipboard.description")}</p>
          </div>
        </div>
        {desktop && (
          <div className="flex flex-wrap gap-2">
            {!status?.enabled && <Button type="primary" onClick={() => setEnableOpen(true)}>{t("zdx.label.clipboard.enable")}</Button>}
            {status?.enabled && !status.paused && <Button icon={<PauseOutlined />} onClick={() => void history.setPaused(true)}>{t("zdx.label.clipboard.pause")}</Button>}
            {status?.enabled && status.paused && <Button icon={<PlayCircleOutlined />} onClick={() => void history.setPaused(false)}>{t("zdx.label.clipboard.resume")}</Button>}
            <Button icon={<SearchOutlined />} onClick={() => void history.readNow()}>{t("zdx.label.clipboard.readNow")}</Button>
            <Button icon={<SettingOutlined />} onClick={() => void openSettings()}>{t("zdx.label.clipboard.settings")}</Button>
            <Button danger onClick={() => setClearOpen(true)}>{t("zdx.label.clipboard.clear")}</Button>
          </div>
        )}
      </div>

      {!desktop && <p className="mt-8 rounded-2xl border border-[#dfe5f0] bg-white p-6 text-sm dark:border-[#34415a] dark:bg-[#1b2537]">{t("zdx.text.clipboard.error.desktop_only")}</p>}

      {desktop && status && (
        <div className="mt-5 text-sm text-[#667491] dark:text-[#aab7cf]">
          <p className="m-0">{clipboardStatusText(t, status.state)}</p>
          {reason && <p className="m-0 mt-1">{reason}</p>}
          <p className="m-0 mt-1">{t("zdx.text.clipboard.storagePath", { path: status.storagePath })}</p>
          {status.platform === "macos" && <p className="m-0 mt-1">{t("zdx.text.clipboard.macosAccess")}</p>}
          {!status.canGlobalShortcut && status.platform.includes("wayland") && <p className="m-0 mt-1">{t("zdx.text.clipboard.waylandShortcut")}</p>}
          {status.skippedSensitive > 0 && <p className="m-0 mt-1">{t("zdx.text.clipboard.skippedSensitive", { count: status.skippedSensitive })}</p>}
          {status.skippedTooLarge > 0 && <p className="m-0 mt-1">{t("zdx.text.clipboard.skippedTooLarge", { count: status.skippedTooLarge })}</p>}
          {status.skippedExcluded > 0 && <p className="m-0 mt-1">{t("zdx.text.clipboard.skippedExcluded", { count: status.skippedExcluded })}</p>}
        </div>
      )}

      {desktop && (
        <>
          <div className="mt-5 flex flex-col gap-3 md:flex-row md:items-center">
            <Input allowClear prefix={<SearchOutlined />} value={history.query} onChange={(event) => history.setQuery(event.target.value)} placeholder={t("zdx.placeholder.clipboard.search")} aria-label={t("zdx.label.clipboard.search")} className="md:max-w-sm" />
            <div className="flex flex-wrap gap-2">
              {FILTERS.map((item) => (
                <button key={item.id} type="button" className={filterClass(history.filter === item.id)} onClick={() => history.setFilter(item.id)}>{t(item.label)}</button>
              ))}
            </div>
          </div>
          {history.loading && history.items.length === 0 && <p className="mt-6 text-sm text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.clipboard.loading")}</p>}
          {!history.loading && history.items.length === 0 && (
            <p className="mt-6 rounded-2xl border border-dashed border-[#d6deeb] p-6 text-sm text-[#667491] dark:border-[#46546b] dark:text-[#aab7cf]">
              {status?.enabled ? (history.query ? t("zdx.text.clipboard.noResults") : t("zdx.text.clipboard.empty")) : t("zdx.text.clipboard.disabled")}
            </p>
          )}
          <div className="mt-4 min-h-0 flex-1 overflow-auto">
            <HistoryList items={history.items} activeId={activeId} onActive={setActiveId} onCopy={(id) => void copy(id)} onPin={(id, pinned) => void history.pinItem(id, pinned)} onDelete={setPendingDelete} />
            {history.nextCursor && <Button className="mt-4" onClick={() => void history.loadMore()}>{t("zdx.label.clipboard.loadMore")}</Button>}
          </div>
          {notice && <p role="status" className="mt-3 text-sm text-[#527a67] dark:text-[#8ed9ad]">{notice}</p>}
          {history.error && history.error.code !== "desktop_only" && <p role="alert" className="mt-3 text-sm text-[#bb3d4a] dark:text-[#ff8b98]">{clipboardErrorText(t, history.error)}</p>}
        </>
      )}

      <p className="mt-3 shrink-0 text-xs leading-5 text-[#7786a0] dark:text-[#aab7cf]">{t("zdx.text.clipboard.privacy")}</p>

      <Modal open={enableOpen} title={t("zdx.text.clipboard.enableTitle")} okText={t("zdx.label.clipboard.enable")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => void enable()} onCancel={() => setEnableOpen(false)}>
        <p>{t("zdx.text.clipboard.enableBody")}</p>
        {status && <p>{t("zdx.text.clipboard.storagePath", { path: status.storagePath })}</p>}
      </Modal>
      <SettingsDialog open={settingsOpen} settings={history.settings} onClose={() => setSettingsOpen(false)} onSave={saveSettings} />
      <Modal open={clearOpen} title={t("zdx.label.clipboard.clear")} okText={t("zdx.label.clipboard.confirm")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => clipboardClear(keepPinned).then(() => history.reload()).then(() => setClearOpen(false)).catch((error) => history.setError(toClipboardError(error)))} onCancel={() => setClearOpen(false)}>
        <p>{t("zdx.text.clipboard.clearConfirm")}</p>
        <label className="mt-3 flex items-center gap-2"><input type="checkbox" checked={keepPinned} onChange={(event) => setKeepPinned(event.target.checked)} />{t("zdx.label.clipboard.keepPinned")}</label>
      </Modal>
      <Modal open={pendingDelete !== null} title={t("zdx.label.clipboard.delete")} okText={t("zdx.label.clipboard.confirm")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => { if (pendingDelete !== null) void history.deleteItem(pendingDelete).then(() => setPendingDelete(null)); }} onCancel={() => setPendingDelete(null)}>
        <p>{t("zdx.text.clipboard.deleteConfirm")}</p>
      </Modal>
    </section>
  );
}
