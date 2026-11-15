import { ScreenshotRequestError, screenshotGetSettings, screenshotPickSaveDir, screenshotStatus, screenshotUpdateSettings, toScreenshotError } from "@/api/screenshot";
import ShortcutField from "@/components/clipboard/ShortcutField";
import type { ShotSettings, ShotStatus } from "@/types/screenshot";
import { isDesktop } from "@/utils/desktop";
import { screenshotErrorText } from "@/utils/screenshotMessage";

export default function ScreenshotSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [draft, setDraft] = useState<ShotSettings | null>(null);
  const [status, setStatus] = useState<ShotStatus | null>(null);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [loading, setLoading] = useState(desktop);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void screenshotStatus().then((current) => {
      if (alive) setStatus(current);
    }).catch(() => undefined);
    void screenshotGetSettings().then((current) => {
      if (!alive) return;
      setDraft({ ...current, format: current.format === "jpeg" ? "jpeg" : "png", scrollAuto: Boolean(current.scrollAuto) });
    }).catch((caught) => {
      if (alive) setError(screenshotErrorText(t, toScreenshotError(caught)));
    }).finally(() => {
      if (alive) setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [desktop, t]);

  async function save() {
    if (!draft) return;
    setSaving(true);
    setSaved(false);
    try {
      const next = await screenshotUpdateSettings({
        ...draft,
        jpegQuality: Number(draft.jpegQuality),
        scrollMaxHeight: Number(draft.scrollMaxHeight),
      });
      setDraft(next);
      setError("");
      setSaved(true);
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
      else setError(t("zdx.text.screenshot.error.unknown"));
    } finally {
      setSaving(false);
    }
  }

  async function chooseFolder() {
    if (!draft) return;
    setChoosing(true);
    setError("");
    try {
      const picked = await screenshotPickSaveDir(t("zdx.label.screenshot.chooseSaveDir"));
      if (picked) setDraft({ ...draft, saveDir: picked });
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
      else setError(t("zdx.text.screenshot.error.unknown"));
    } finally {
      setChoosing(false);
    }
  }

  return (
    <Flex vertical gap={16}>
      <Flex justify="space-between" align="flex-start" gap={16} wrap>
        <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.screenshot.settingsDescription")}</Typography.Paragraph>
        {desktop && <Button type="primary" htmlType="submit" form="screenshot-settings" loading={saving} disabled={!draft}>{t("zdx.label.screenshot.save")}</Button>}
      </Flex>
      {saved && <Alert type="success" showIcon message={t("zdx.text.screenshot.settingsSaved")} />}
      {error && <Alert type="error" showIcon message={error} />}
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.screenshot.desktopOnly")} />}
      {desktop && loading && (
        <Flex align="center" gap={8}>
          <Spin size="small" />
          <Typography.Text type="secondary">{t("zdx.text.screenshot.loading")}</Typography.Text>
        </Flex>
      )}
      {desktop && draft && (
        <Form id="screenshot-settings" layout="vertical" requiredMark={false} onFinish={() => void save()} className="max-w-xl">
          <Form.Item label={t("zdx.label.screenshot.shortcut")} extra={t("zdx.text.screenshot.shortcutHelp")}>
            <ShortcutField
              value={draft.shortcut}
              macos={status?.platform === "macos"}
              label={t("zdx.label.screenshot.shortcut")}
              recordingText={t("zdx.text.screenshot.shortcutRecording")}
              modifierText={t("zdx.text.screenshot.shortcutNeedModifier")}
              onChange={(shortcut) => setDraft({ ...draft, shortcut })}
            />
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.recordShortcut")} extra={t("zdx.text.screenshot.recordShortcut")}>
            <ShortcutField
              value={draft.recordShortcut}
              macos={status?.platform === "macos"}
              label={t("zdx.label.screenshot.recordShortcut")}
              recordingText={t("zdx.text.screenshot.shortcutRecording")}
              modifierText={t("zdx.text.screenshot.shortcutNeedModifier")}
              onChange={(recordShortcut) => setDraft({ ...draft, recordShortcut })}
            />
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.saveDir")} extra={t("zdx.text.screenshot.saveDir")}>
            <Flex gap={8}>
              <Input className="min-w-0 flex-1" value={draft.saveDir} aria-label={t("zdx.label.screenshot.saveDir")} placeholder={t("zdx.placeholder.screenshot.saveDir")} onChange={(event) => setDraft({ ...draft, saveDir: event.target.value })} />
              <Button htmlType="button" loading={choosing} onClick={() => void chooseFolder()}>{t("zdx.label.screenshot.chooseSaveDir")}</Button>
            </Flex>
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.format")}>
            <Select
              aria-label={t("zdx.label.screenshot.format")}
              value={draft.format}
              onChange={(format) => {
                if (format === "png" || format === "jpeg") setDraft({ ...draft, format });
              }}
              options={[
                { value: "png", label: t("zdx.label.screenshot.png") },
                { value: "jpeg", label: t("zdx.label.screenshot.jpeg") },
              ]}
            />
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.jpegQuality")}>
            <InputNumber
              min={60}
              max={100}
              disabled={draft.format !== "jpeg"}
              style={{ width: "100%" }}
              value={draft.jpegQuality}
              onChange={(value) => setDraft({ ...draft, jpegQuality: typeof value === "number" ? value : draft.jpegQuality })}
            />
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.hideSelf")} extra={t("zdx.text.screenshot.hideSelf")}>
            <Switch aria-label={t("zdx.label.screenshot.hideSelf")} checked={draft.hideSelf} onChange={(hideSelf) => setDraft({ ...draft, hideSelf })} />
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.scrollAuto")} extra={t("zdx.text.screenshot.scrollAuto")}>
            <Switch aria-label={t("zdx.label.screenshot.scrollAuto")} checked={draft.scrollAuto} onChange={(scrollAuto) => setDraft({ ...draft, scrollAuto })} />
          </Form.Item>
          <Form.Item label={t("zdx.label.screenshot.scrollHeight")} extra={t("zdx.text.screenshot.scrollHeight")}>
            <InputNumber
              min={2000}
              max={60000}
              style={{ width: "100%" }}
              value={draft.scrollMaxHeight}
              onChange={(value) => setDraft({ ...draft, scrollMaxHeight: typeof value === "number" ? value : draft.scrollMaxHeight })}
            />
          </Form.Item>
        </Form>
      )}
    </Flex>
  );
}
