import { ClipboardRequestError, clipboardGetSettings, clipboardStatus, clipboardUpdateSettings, toClipboardError } from "@/api/clipboard";
import ExcludedAppsField from "@/components/clipboard/ExcludedAppsField";
import ShortcutField from "@/components/clipboard/ShortcutField";
import type { ClipboardSettings, ClipboardStatus } from "@/types/clipboard";
import { toSettingsUpdate } from "@/types/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";
import { bytesToMegabytes, megabytesToBytes } from "@/utils/time";

export default function ClipboardSettingsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const desktop = isDesktop();
  const [draft, setDraft] = useState<ClipboardSettings | null>(null);
  const [status, setStatus] = useState<ClipboardStatus | null>(null);
  const [textMb, setTextMb] = useState<number | null>(1);
  const [imageMb, setImageMb] = useState<number | null>(20);
  const [excluded, setExcluded] = useState("");
  const [excludedApps, setExcludedApps] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [confirmCount, setConfirmCount] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(desktop);
  const translate = useRef(t);
  translate.current = t;

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void clipboardStatus().then((current) => {
      if (alive) setStatus(current);
    }).catch(() => undefined);
    void clipboardGetSettings()
      .then((current) => {
        if (!alive) return;
        setDraft(current);
        setTextMb(Number(bytesToMegabytes(current.maxTextBytes)));
        setImageMb(Number(bytesToMegabytes(current.maxImageBytes)));
        setExcluded(current.excludedApps.join("\n"));
        setExcludedApps(current.excludedApps);
      })
      .catch((caught) => {
        if (!alive) return;
        setError(clipboardErrorText(translate.current, toClipboardError(caught)));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [desktop]);

  async function save(confirm: boolean) {
    if (!draft) return;
    const maxItems = Number(draft.maxItems);
    const retentionDays = Number(draft.retentionDays);
    const maxTextBytes = megabytesToBytes(textMb == null ? "" : String(textMb));
    const maxImageBytes = megabytesToBytes(imageMb == null ? "" : String(imageMb));
    if (!Number.isFinite(maxItems) || !Number.isFinite(retentionDays) || !Number.isFinite(maxTextBytes) || !Number.isFinite(maxImageBytes)) {
      setError(t("zdx.text.clipboard.error.invalid_input"));
      return;
    }
    const next: ClipboardSettings = {
      ...draft,
      maxItems,
      retentionDays,
      maxTextBytes,
      maxImageBytes,
      excludedApps: status?.platform.includes("macos") ? excludedApps : excluded.split("\n"),
    };
    setSaving(true);
    try {
      await clipboardUpdateSettings(toSettingsUpdate(next, confirm));
      navigate("/clipboard");
    } catch (caught) {
      if (caught instanceof ClipboardRequestError && caught.code === "needs_confirm") {
        setConfirmCount(Number(caught.params.count ?? 0));
        setError("");
      } else if (caught instanceof ClipboardRequestError) {
        setError(clipboardErrorText(t, caught));
      } else {
        setError(t("zdx.text.clipboard.error.unknown"));
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Flex vertical gap={16} className="min-h-0 flex-1 overflow-hidden">
      <Flex justify="space-between" align="flex-start" gap={16} wrap>
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>{t("zdx.label.clipboard.settings")}</Typography.Title>
          <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.clipboard.settingsDescription")}</Typography.Paragraph>
          {status && <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.clipboard.storagePath", { path: status.storagePath })}</Typography.Paragraph>}
        </div>
        <Space>
          <Button onClick={() => navigate("/clipboard")}>{t("zdx.label.clipboard.back")}</Button>
          {desktop && <Button type="primary" htmlType="submit" form="clipboard-settings" loading={saving} disabled={!draft}>{t("zdx.label.clipboard.save")}</Button>}
        </Space>
      </Flex>

      {confirmCount !== null && (
        <Alert
          type="warning"
          showIcon
          message={t("zdx.text.clipboard.confirmPrune", { count: confirmCount })}
          action={<Button size="small" type="primary" onClick={() => void save(true)}>{t("zdx.label.clipboard.confirm")}</Button>}
        />
      )}
      {status?.platform.includes("macos") && <Alert type="info" showIcon message={t("zdx.text.clipboard.macosAccess")} />}
      {error && <Alert type="error" showIcon message={error} />}
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.clipboard.error.desktop_only")} />}
      {desktop && loading && (
        <Flex align="center" gap={8}>
          <Spin size="small" />
          <Typography.Text type="secondary">{t("zdx.text.clipboard.loading")}</Typography.Text>
        </Flex>
      )}

      {desktop && draft && (
        <Form id="clipboard-settings" layout="vertical" requiredMark={false} onFinish={() => void save(false)} className="min-h-0 max-w-xl flex-1 overflow-auto">
          <Form.Item label={t("zdx.label.clipboard.recordText")}><Switch checked={draft.recordText} onChange={(checked) => setDraft({ ...draft, recordText: checked })} /></Form.Item>
          <Form.Item label={t("zdx.label.clipboard.recordImage")}><Switch checked={draft.recordImage} onChange={(checked) => setDraft({ ...draft, recordImage: checked })} /></Form.Item>
          <Form.Item label={t("zdx.label.clipboard.recordFiles")}><Switch checked={draft.recordFiles} onChange={(checked) => setDraft({ ...draft, recordFiles: checked })} /></Form.Item>
          <Form.Item label={t("zdx.label.clipboard.skipSensitive")}><Switch checked={draft.skipSensitive} onChange={(checked) => setDraft({ ...draft, skipSensitive: checked })} /></Form.Item>
          <Form.Item label={t("zdx.label.clipboard.maxItems")}>
            <InputNumber min={1} max={5000} style={{ width: "100%" }} value={draft.maxItems} onChange={(value) => setDraft({ ...draft, maxItems: Number(value) })} />
          </Form.Item>
          <Form.Item label={t("zdx.label.clipboard.retentionDays")} extra={t("zdx.text.clipboard.retentionZero")}>
            <InputNumber min={0} max={3650} style={{ width: "100%" }} value={draft.retentionDays} onChange={(value) => setDraft({ ...draft, retentionDays: Number(value) })} />
          </Form.Item>
          <Form.Item label={t("zdx.label.clipboard.maxText")}>
            <InputNumber min={0} style={{ width: "100%" }} value={textMb} onChange={(value) => setTextMb(typeof value === "number" ? value : null)} />
          </Form.Item>
          <Form.Item label={t("zdx.label.clipboard.maxImage")}>
            <InputNumber min={0} style={{ width: "100%" }} value={imageMb} onChange={(value) => setImageMb(typeof value === "number" ? value : null)} />
          </Form.Item>
          <Form.Item label={t("zdx.label.clipboard.shortcut")} extra={t("zdx.text.clipboard.shortcutHelp")}>
            <ShortcutField value={draft.shortcut} macos={status?.platform.includes("macos") ?? false} onChange={(shortcut) => setDraft({ ...draft, shortcut })} />
          </Form.Item>
          <Form.Item
            label={t("zdx.label.clipboard.excludedApps")}
            extra={
              <Flex vertical gap={4}>
                {status?.platform.includes("macos") ? <span>{t("zdx.text.clipboard.excludedPicker")}</span> : status ? <span>{t("zdx.text.clipboard.excludedHelp")}</span> : null}
                {status?.platform.includes("wayland") && <span>{t("zdx.text.clipboard.excludedWayland")}</span>}
              </Flex>
            }
          >
            {status == null ? (
              <Spin size="small" />
            ) : status.platform.includes("macos") ? (
              <ExcludedAppsField value={excludedApps} onChange={setExcludedApps} />
            ) : (
              <Input.TextArea value={excluded} onChange={(event) => setExcluded(event.target.value)} placeholder={t("zdx.placeholder.clipboard.excludedApps")} rows={4} />
            )}
          </Form.Item>
        </Form>
      )}
    </Flex>
  );
}
