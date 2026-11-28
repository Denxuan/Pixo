import { ScreenshotRequestError, screenshotGetSettings, screenshotPickSaveDir, screenshotStatus, screenshotUpdateSettings, toScreenshotError } from "@/api/screenshot";
import ShortcutField from "@/components/clipboard/ShortcutField";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import { useAutosave } from "@/hooks/useAutosave";
import type { ShotSettings, ShotStatus } from "@/types/screenshot";
import { isDesktop } from "@/utils/desktop";
import { screenshotErrorText } from "@/utils/screenshotMessage";

function screenshotReady(draft: ShotSettings): boolean {
  const jpegQuality = Number(draft.jpegQuality);
  const scrollMaxHeight = Number(draft.scrollMaxHeight);
  const shortcut = draft.shortcut.trim();
  const recordShortcut = draft.recordShortcut.trim();
  return draft.saveDir.trim().length > 0
    && draft.saveDir.trim().length <= 1024
    && (draft.format === "png" || draft.format === "jpeg")
    && Number.isInteger(jpegQuality)
    && jpegQuality >= 60
    && jpegQuality <= 100
    && Number.isInteger(scrollMaxHeight)
    && scrollMaxHeight >= 2000
    && scrollMaxHeight <= 60000
    && shortcut.length > 0
    && recordShortcut.length > 0
    && shortcut !== recordShortcut;
}

export default function ScreenshotSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [draft, setDraft] = useState<ShotSettings | null>(null);
  const [status, setStatus] = useState<ShotStatus | null>(null);
  const [error, setError] = useState("");
  const [choosing, setChoosing] = useState(false);
  const [loading, setLoading] = useState(desktop);
  const translate = useRef(t);
  translate.current = t;

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
      if (alive) setError(screenshotErrorText(translate.current, toScreenshotError(caught)));
    }).finally(() => {
      if (alive) setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [desktop]);

  const payload = useMemo(() => {
    if (!draft) return null;
    return {
      ...draft,
      jpegQuality: Number(draft.jpegQuality),
      scrollMaxHeight: Number(draft.scrollMaxHeight),
    };
  }, [draft]);

  useAutosave(payload, async (next, isCurrent) => {
    if (!screenshotReady(next)) {
      if (isCurrent()) setError(t("zdx.text.screenshot.settingsInvalid"));
      return false;
    }
    try {
      await screenshotUpdateSettings(next);
      if (!isCurrent()) return false;
      setError("");
      return true;
    } catch (caught) {
      if (!isCurrent()) return false;
      if (caught instanceof ScreenshotRequestError && caught.code === "invalid_input") setError(t("zdx.text.screenshot.settingsInvalid"));
      else if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
      else setError(t("zdx.text.screenshot.error.unknown"));
      return false;
    }
  });

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
      {error && <Alert type="error" showIcon message={error} />}
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.screenshot.desktopOnly")} />}
      {desktop && loading && (
        <Flex align="center" gap={8}>
          <Spin size="small" />
          <Typography.Text type="secondary">{t("zdx.text.screenshot.loading")}</Typography.Text>
        </Flex>
      )}
      {desktop && draft && (
        <Flex vertical gap={24}>
            <SettingGroup title={t("zdx.label.screenshot.groupShortcuts")}>
              <SettingRow label={t("zdx.label.screenshot.shortcut")} description={t("zdx.text.screenshot.shortcutHelp")}>
                <ShortcutField
                  value={draft.shortcut}
                  macos={status?.platform === "macos"}
                  label={t("zdx.label.screenshot.shortcut")}
                  recordingText={t("zdx.text.screenshot.shortcutRecording")}
                  modifierText={t("zdx.text.screenshot.shortcutNeedModifier")}
                  onChange={(shortcut) => setDraft({ ...draft, shortcut })}
                />
              </SettingRow>
              <SettingRow label={t("zdx.label.screenshot.recordShortcut")} description={t("zdx.text.screenshot.recordShortcut")}>
                <ShortcutField
                  value={draft.recordShortcut}
                  macos={status?.platform === "macos"}
                  label={t("zdx.label.screenshot.recordShortcut")}
                  recordingText={t("zdx.text.screenshot.shortcutRecording")}
                  modifierText={t("zdx.text.screenshot.shortcutNeedModifier")}
                  onChange={(recordShortcut) => setDraft({ ...draft, recordShortcut })}
                />
              </SettingRow>
            </SettingGroup>

            <SettingGroup title={t("zdx.label.screenshot.groupOutput")}>
              <SettingRow stacked label={t("zdx.label.screenshot.saveDir")} description={t("zdx.text.screenshot.saveDir")}>
                <Flex gap={8}>
                  <Input className="min-w-0 flex-1" value={draft.saveDir} aria-label={t("zdx.label.screenshot.saveDir")} placeholder={t("zdx.placeholder.screenshot.saveDir")} onChange={(event) => setDraft({ ...draft, saveDir: event.target.value })} />
                  <Button htmlType="button" loading={choosing} onClick={() => void chooseFolder()}>{t("zdx.label.screenshot.chooseSaveDir")}</Button>
                </Flex>
              </SettingRow>
              <SettingRow label={t("zdx.label.screenshot.format")}>
                <Segmented
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
              </SettingRow>
              <SettingRow label={t("zdx.label.screenshot.jpegQuality")}>
                <InputNumber
                  aria-label={t("zdx.label.screenshot.jpegQuality")}
                  min={60}
                  max={100}
                  disabled={draft.format !== "jpeg"}
                  style={{ width: 140 }}
                  value={draft.jpegQuality}
                  onChange={(value) => {
                    if (typeof value === "number") setDraft({ ...draft, jpegQuality: value });
                  }}
                />
              </SettingRow>
            </SettingGroup>

            <SettingGroup title={t("zdx.label.screenshot.groupCapture")}>
              <SettingRow label={t("zdx.label.screenshot.hideSelf")} description={t("zdx.text.screenshot.hideSelf")}>
                <Switch aria-label={t("zdx.label.screenshot.hideSelf")} checked={draft.hideSelf} onChange={(hideSelf) => setDraft({ ...draft, hideSelf })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.screenshot.scrollAuto")} description={t("zdx.text.screenshot.scrollAuto")}>
                <Switch aria-label={t("zdx.label.screenshot.scrollAuto")} checked={draft.scrollAuto} onChange={(scrollAuto) => setDraft({ ...draft, scrollAuto })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.screenshot.scrollHeight")} description={t("zdx.text.screenshot.scrollHeight")}>
                <InputNumber
                  aria-label={t("zdx.label.screenshot.scrollHeight")}
                  min={2000}
                  max={60000}
                  style={{ width: 140 }}
                  value={draft.scrollMaxHeight}
                  onChange={(value) => {
                    if (typeof value === "number") setDraft({ ...draft, scrollMaxHeight: value });
                  }}
                />
              </SettingRow>
            </SettingGroup>
          </Flex>
      )}
    </Flex>
  );
}
