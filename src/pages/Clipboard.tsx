import { ClipboardRequestError, clipboardClear, toClipboardError } from "@/api/clipboard";
import HistoryList from "@/components/clipboard/HistoryList";
import { useClipboardHistory } from "@/hooks/useClipboard";
import type { ClipFilter } from "@/types/clipboard";
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

function isFilter(value: string | number): value is ClipFilter {
  return value === "all" || value === "text" || value === "image" || value === "files" || value === "pinned";
}

export default function ClipboardPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const history = useClipboardHistory();
  const [activeId, setActiveId] = useState<number | null>(null);
  const [notice, setNotice] = useState("");
  const [enableOpen, setEnableOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const [keepPinned, setKeepPinned] = useState(true);
  const [pendingDelete, setPendingDelete] = useState<number | null>(null);
  const desktop = isDesktop();

  useEffect(() => {
    const state = location.state as { enable?: boolean } | null;
    if (!state?.enable) return;
    setEnableOpen(true);
    navigate("/clipboard", { replace: true, state: null });
  }, [location.state, navigate]);

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

  const status = history.status;
  const reason = clipboardReasonText(t, status?.reason ?? null);
  const details = status ? [
    reason,
    !status.canGlobalShortcut && status.platform.includes("wayland") ? t("zdx.text.clipboard.waylandShortcut") : "",
    status.skippedSensitive > 0 ? t("zdx.text.clipboard.skippedSensitive", { count: status.skippedSensitive }) : "",
    status.skippedTooLarge > 0 ? t("zdx.text.clipboard.skippedTooLarge", { count: status.skippedTooLarge }) : "",
    status.skippedExcluded > 0 ? t("zdx.text.clipboard.skippedExcluded", { count: status.skippedExcluded }) : "",
  ].filter((line) => line.length > 0) : [];
  const emptyText = status?.enabled ? (history.query ? t("zdx.text.clipboard.noResults") : t("zdx.text.clipboard.empty")) : t("zdx.text.clipboard.disabled");

  return (
    <Flex vertical gap={16} className="h-full min-h-0 flex-1 overflow-hidden">
      <Flex justify="space-between" align="flex-start" gap={16} wrap>
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>{t("zdx.label.clipboard.title")}</Typography.Title>
          <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.clipboard.description")}</Typography.Paragraph>
        </div>
        {desktop && (
          <Space wrap>
            {!status?.enabled && <Button type="primary" onClick={() => setEnableOpen(true)}>{t("zdx.label.clipboard.enable")}</Button>}
            {status?.enabled && !status.paused && <Button icon={<PauseOutlined />} onClick={() => void history.setPaused(true)}>{t("zdx.label.clipboard.pause")}</Button>}
            {status?.enabled && status.paused && <Button icon={<PlayCircleOutlined />} onClick={() => void history.setPaused(false)}>{t("zdx.label.clipboard.resume")}</Button>}
            <Button icon={<SearchOutlined />} onClick={() => void history.readNow()}>{t("zdx.label.clipboard.readNow")}</Button>
            <Button icon={<SettingOutlined />} onClick={() => navigate("/settings/clipboard")}>{t("zdx.label.clipboard.settings")}</Button>
            <Button danger onClick={() => setClearOpen(true)}>{t("zdx.label.clipboard.clear")}</Button>
          </Space>
        )}
      </Flex>

      {!desktop && <Alert type="info" showIcon message={t("zdx.text.clipboard.error.desktop_only")} />}

      {desktop && status && (
        <Alert
          type="info"
          showIcon
          message={clipboardStatusText(t, status.state)}
          description={details.length > 0 ? <Flex vertical gap={4}>{details.map((line) => <span key={line}>{line}</span>)}</Flex> : undefined}
        />
      )}

      {desktop && (
        <>
          <Flex gap={12} wrap align="center">
            <Input
              allowClear
              prefix={<SearchOutlined />}
              value={history.query}
              onChange={(event) => history.setQuery(event.target.value)}
              placeholder={t("zdx.placeholder.clipboard.search")}
              aria-label={t("zdx.label.clipboard.search")}
              style={{ maxWidth: 360 }}
            />
            <Segmented
              value={history.filter}
              options={FILTERS.map((item) => ({ label: t(item.label), value: item.id }))}
              onChange={(value) => {
                if (isFilter(value)) history.setFilter(value);
              }}
            />
          </Flex>
          <div className="min-h-0 flex-1 overflow-auto">
            {history.loading && history.items.length === 0 && (
              <Flex align="center" gap={8}>
                <Spin size="small" />
                <Typography.Text type="secondary">{t("zdx.text.clipboard.loading")}</Typography.Text>
              </Flex>
            )}
            {!history.loading && history.items.length === 0 && <Empty description={emptyText} />}
            {history.items.length > 0 && (
              <>
                <HistoryList items={history.items} activeId={activeId} onActive={setActiveId} onCopy={(id) => void copy(id)} onPin={(id, pinned) => void history.pinItem(id, pinned)} onDelete={setPendingDelete} />
                {history.nextCursor && <Button onClick={() => void history.loadMore()}>{t("zdx.label.clipboard.loadMore")}</Button>}
              </>
            )}
          </div>
          {notice && <Alert type="success" showIcon message={notice} />}
          {history.error && history.error.code !== "desktop_only" && <Alert type="error" showIcon message={clipboardErrorText(t, history.error)} />}
        </>
      )}

      <Typography.Text type="secondary" className="mt-auto shrink-0">{t("zdx.text.clipboard.privacy")}</Typography.Text>

      <Modal open={enableOpen} title={t("zdx.text.clipboard.enableTitle")} okText={t("zdx.label.clipboard.enable")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => void enable()} onCancel={() => setEnableOpen(false)}>
        <Typography.Paragraph>{t("zdx.text.clipboard.enableBody")}</Typography.Paragraph>
        {status && <Typography.Paragraph>{t("zdx.text.clipboard.storagePath", { path: status.storagePath })}</Typography.Paragraph>}
      </Modal>
      <Modal open={clearOpen} title={t("zdx.label.clipboard.clear")} okText={t("zdx.label.clipboard.confirm")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => clipboardClear(keepPinned).then(() => history.reload()).then(() => setClearOpen(false)).catch((error) => history.setError(toClipboardError(error)))} onCancel={() => setClearOpen(false)}>
        <Typography.Paragraph>{t("zdx.text.clipboard.clearConfirm")}</Typography.Paragraph>
        <Checkbox checked={keepPinned} onChange={(event) => setKeepPinned(event.target.checked)}>{t("zdx.label.clipboard.keepPinned")}</Checkbox>
      </Modal>
      <Modal open={pendingDelete !== null} title={t("zdx.label.clipboard.delete")} okText={t("zdx.label.clipboard.confirm")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => { if (pendingDelete !== null) void history.deleteItem(pendingDelete).then(() => setPendingDelete(null)); }} onCancel={() => setPendingDelete(null)}>
        <Typography.Paragraph>{t("zdx.text.clipboard.deleteConfirm")}</Typography.Paragraph>
      </Modal>
    </Flex>
  );
}
