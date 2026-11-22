import { ClipboardRequestError, clipboardGet, clipboardPlaceText, clipboardSetLabel } from "@/api/clipboard";
import HistoryList from "@/components/clipboard/HistoryList";
import Page from "@/components/Page";
import { useClipboardHistory } from "@/hooks/useClipboard";
import { CLIP_FILTERS, isClipFilter, toSettingsUpdate } from "@/types/clipboard";
import { clipboardErrorText, clipboardReasonText, clipboardStatusText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";
import { formatJson } from "@/utils/json";
import { DEFAULT_DELETE_SHORTCUT, DEFAULT_PIN_SHORTCUT, eventMatchesShortcut } from "@/utils/shortcut";

export default function ClipboardPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const history = useClipboardHistory();
  const [activeId, setActiveId] = useState<number | null>(null);
  const [notice, setNotice] = useState("");
  const [noticeTone, setNoticeTone] = useState<"success" | "warning">("success");
  const [clearAll, setClearAll] = useState(false);
  const [enableOpen, setEnableOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<number | null>(null);
  const [labelDraft, setLabelDraft] = useState("");
  const desktop = isDesktop();
  const active = history.items.find((item) => item.id === activeId) ?? null;

  useEffect(() => {
    const state = location.state as { enable?: boolean } | null;
    if (!state?.enable) return;
    setEnableOpen(true);
    navigate("/clipboard", { replace: true, state: null });
  }, [location.state, navigate]);

  useEffect(() => {
    setLabelDraft(active?.label ?? "");
  }, [active?.id, active?.label]);

  useEffect(() => {
    if (history.items.length === 0) {
      setActiveId(null);
      return;
    }
    if (!history.items.some((item) => item.id === activeId)) setActiveId(history.items[0].id);
  }, [history.items, activeId]);

  async function saveLabel() {
    if (!active?.pinned) return;
    const next = labelDraft.trim();
    if (next === (active.label ?? "")) return;
    try {
      await clipboardSetLabel(active.id, next);
      await history.reload();
    } catch (error) {
      if (error instanceof ClipboardRequestError) {
        setNoticeTone("warning");
        setNotice(clipboardErrorText(t, error));
      }
    }
  }

  async function transform(kind: "format" | "compact" | "trim") {
    if (!active) return;
    try {
      const detail = await clipboardGet(active.id);
      const text = detail.text ?? "";
      const next = kind === "trim" ? text.trim() : formatJson(text, kind === "compact");
      if (!next) return;
      await clipboardPlaceText(next);
      setNoticeTone("success");
      setNotice(t("zdx.text.clipboard.transformed"));
      await history.reload();
    } catch (error) {
      setNoticeTone("warning");
      setNotice(error instanceof ClipboardRequestError ? clipboardErrorText(t, error) : t("zdx.text.clipboard.jsonInvalid"));
    }
  }

  async function copy(id: number) {
    try {
      const result = await history.copyItem(id);
      setNoticeTone(result.missing.length > 0 ? "warning" : "success");
      setNotice(result.missing.length > 0 ? t("zdx.text.clipboard.fileMissing", { count: result.missing.length }) : t("zdx.text.clipboard.copied"));
    } catch (error) {
      if (error instanceof ClipboardRequestError) {
        setNoticeTone("warning");
        setNotice(clipboardErrorText(t, error));
      }
    }
  }

  async function enable() {
    const current = history.settings ?? await history.loadSettings();
    await history.saveSettings(toSettingsUpdate({ ...current, enabled: true }));
    setEnableOpen(false);
  }

  const pinShortcut = history.settings?.pinShortcut || DEFAULT_PIN_SHORTCUT;
  const deleteShortcut = history.settings?.deleteShortcut || DEFAULT_DELETE_SHORTCUT;

  function onKeyDown(event: React.KeyboardEvent) {
    const item = history.items.find((entry) => entry.id === activeId);
    if (!item) return;
    if (eventMatchesShortcut(event, pinShortcut)) {
      event.preventDefault();
      void history.pinItem(item.id, !item.pinned);
    } else if (eventMatchesShortcut(event, deleteShortcut) && pendingDelete === null) {
      event.preventDefault();
      setPendingDelete(item.id);
    }
  }

  const status = history.status;
  const reason = clipboardReasonText(t, status?.reason ?? null);
  const details = status ? [
    reason,
    !status.canGlobalShortcut && status.platform.includes("wayland") ? t("zdx.text.clipboard.waylandShortcut") : "",
    status.skippedSensitive > 0 ? t("zdx.text.clipboard.skippedSensitive", { count: status.skippedSensitive }) : "",
    status.skippedTooLarge > 0 ? t("zdx.text.clipboard.skippedTooLarge", { count: status.skippedTooLarge }) : "",
    status.skippedExcluded > 0 ? t("zdx.text.clipboard.skippedExcluded", { count: status.skippedExcluded }) : "",
    status.skippedIgnored > 0 ? t("zdx.text.clipboard.skippedIgnored", { count: status.skippedIgnored }) : "",
  ].filter((line) => line.length > 0) : [];
  const emptyText = status?.enabled ? (history.query ? t("zdx.text.clipboard.noResults") : t("zdx.text.clipboard.empty")) : t("zdx.text.clipboard.disabled");

  return (
    <Page
      fill
      title={t("zdx.label.clipboard.title")}
      description={t("zdx.text.clipboard.description")}
      extra={desktop && (
        <>
          {!status?.enabled && <Button type="primary" onClick={() => setEnableOpen(true)}>{t("zdx.label.clipboard.enable")}</Button>}
          {status?.enabled && !status.paused && <Button icon={<PauseOutlined />} onClick={() => void history.setPaused(true)}>{t("zdx.label.clipboard.pause")}</Button>}
          {status?.enabled && status.paused && <Button icon={<PlayCircleOutlined />} onClick={() => void history.setPaused(false)}>{t("zdx.label.clipboard.resume")}</Button>}
          <Tooltip title={t("zdx.label.clipboard.readNow")}>
            <Button icon={<ImportOutlined />} aria-label={t("zdx.label.clipboard.readNow")} onClick={() => void history.readNow()} />
          </Tooltip>
          <Tooltip title={t("zdx.label.clipboard.settings")}>
            <Button icon={<SettingOutlined />} aria-label={t("zdx.label.clipboard.settings")} onClick={() => navigate("/settings/clipboard")} />
          </Tooltip>
          <Popconfirm
            title={t("zdx.label.clipboard.clear")}
            description={t(clearAll ? "zdx.text.clipboard.clearAllConfirm" : "zdx.text.clipboard.clearConfirm")}
            okText={t("zdx.label.clipboard.confirm")}
            cancelText={t("zdx.label.clipboard.cancel")}
            okButtonProps={{ danger: true }}
            placement="bottomRight"
            onConfirm={() => history.clearHistory(!clearAll).then(() => { setNoticeTone("success"); setNotice(t(clearAll ? "zdx.text.clipboard.clearedAll" : "zdx.text.clipboard.cleared")); })}
          >
            <Button danger icon={<ClearOutlined />} onMouseDown={(event) => setClearAll(event.altKey && event.shiftKey)}>{t("zdx.label.clipboard.clear")}</Button>
          </Popconfirm>
        </>
      )}
    >
      <Flex vertical gap={16} className="min-h-0 flex-1 overflow-hidden" onKeyDown={onKeyDown}>
        {!desktop && <Alert type="info" showIcon message={t("zdx.text.clipboard.error.desktop_only")} />}

        {desktop && status && (
          <Alert
            type={status.enabled && !status.paused ? "success" : "info"}
            showIcon
            message={clipboardStatusText(t, status.state)}
            description={details.length > 0 ? <Flex vertical gap={4}>{details.map((line) => <span key={line}>{line}</span>)}</Flex> : undefined}
          />
        )}

        {desktop && (
          <>
            <Flex gap={12} wrap align="center" justify="space-between">
              <Input
                allowClear
                prefix={<SearchOutlined />}
                value={history.query}
                onChange={(event) => history.setQuery(event.target.value)}
                placeholder={t("zdx.placeholder.clipboard.search")}
                aria-label={t("zdx.label.clipboard.search")}
                className="min-w-[200px] flex-[1_1_240px]"
                style={{ maxWidth: 360 }}
              />
              <Segmented
                value={history.filter}
                options={CLIP_FILTERS.map((item) => ({ label: t(item.label), value: item.id }))}
                onChange={(value) => {
                  if (isClipFilter(value)) history.setFilter(value);
                }}
              />
            </Flex>
            {active?.pinned && (
              <Input
                aria-label={t("zdx.label.clipboard.pinTitle")}
                placeholder={t("zdx.placeholder.clipboard.pinTitle")}
                value={labelDraft}
                onChange={(event) => setLabelDraft(event.target.value)}
                onBlur={() => void saveLabel()}
                onPressEnter={() => void saveLabel()}
                style={{ maxWidth: 360 }}
              />
            )}
            {active?.kind === "text" && (
              <Flex gap={8} wrap>
                <Button onClick={() => void transform("format")}>{t("zdx.label.clipboard.formatJson")}</Button>
                <Button onClick={() => void transform("compact")}>{t("zdx.label.clipboard.compactJson")}</Button>
                <Button onClick={() => void transform("trim")}>{t("zdx.label.clipboard.trimText")}</Button>
                <Button onClick={() => void history.copyItem(active.id, { plain: true }).then(() => { setNoticeTone("success"); setNotice(t("zdx.text.clipboard.copied")); }).catch((error) => {
                  if (error instanceof ClipboardRequestError) { setNoticeTone("warning"); setNotice(clipboardErrorText(t, error)); }
                })}>{t("zdx.label.clipboard.pastePlain")}</Button>
              </Flex>
            )}
            <div className="-mr-2 min-h-0 flex-1 overflow-auto pr-2">
              {history.loading && history.items.length === 0 && (
                <Flex align="center" gap={8}>
                  <Spin size="small" />
                  <Typography.Text type="secondary">{t("zdx.text.clipboard.loading")}</Typography.Text>
                </Flex>
              )}
              {!history.loading && history.items.length === 0 && <Empty className="py-12" image={Empty.PRESENTED_IMAGE_SIMPLE} description={emptyText} />}
              {history.items.length > 0 && (
                <>
                  <HistoryList items={history.items} activeId={activeId} onActive={setActiveId} onCopy={(id) => void copy(id)} onPin={(id, pinned) => void history.pinItem(id, pinned)} onDelete={setPendingDelete} />
                  {history.nextCursor && <Flex justify="center" className="py-2"><Button type="text" onClick={() => void history.loadMore()}>{t("zdx.label.clipboard.loadMore")}</Button></Flex>}
                </>
              )}
            </div>
            {notice && <Alert type={noticeTone} showIcon closable message={notice} onClose={() => setNotice("")} />}
            {history.error && history.error.code !== "desktop_only" && <Alert type="error" showIcon message={clipboardErrorText(t, history.error)} />}
          </>
        )}

        <Typography.Text type="secondary" className="mt-auto shrink-0 text-xs">{t("zdx.text.clipboard.privacy")}</Typography.Text>

        <Modal open={enableOpen} title={t("zdx.text.clipboard.enableTitle")} okText={t("zdx.label.clipboard.enable")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => void enable()} onCancel={() => setEnableOpen(false)}>
          <Typography.Paragraph>{t("zdx.text.clipboard.enableBody")}</Typography.Paragraph>
          {status && <Typography.Paragraph>{t("zdx.text.clipboard.storagePath", { path: status.storagePath })}</Typography.Paragraph>}
        </Modal>
        <Modal open={pendingDelete !== null} title={t("zdx.label.clipboard.delete")} okText={t("zdx.label.clipboard.confirm")} cancelText={t("zdx.label.clipboard.cancel")} onOk={() => { if (pendingDelete !== null) void history.deleteItem(pendingDelete).then(() => setPendingDelete(null)); }} onCancel={() => setPendingDelete(null)}>
          <Typography.Paragraph>{t("zdx.text.clipboard.deleteConfirm")}</Typography.Paragraph>
        </Modal>
      </Flex>
    </Page>
  );
}
