import { clipboardGetSettings, clipboardSetEnabled, clipboardShowPanel } from "@/api/clipboard";
import { permissionsOpen, permissionsStatus } from "@/api/permissions";
import { ScreenshotRequestError, openScreenRecordingSettings, screenshotRequestAccess, screenshotStart, screenshotStatus } from "@/api/screenshot";
import { useSettingsStore } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";
import { screenshotErrorText } from "@/utils/screenshotMessage";
import { formatShortcut, isApplePlatform } from "@/utils/shortcut";

type Block = "" | "accessibility" | "screen" | "error";

export default function FirstRun() {
  const { t } = useTranslation();
  const introduced = useSettingsStore((state) => state.introduced);
  const setIntroduced = useSettingsStore((state) => state.setIntroduced);
  const hydrated = useSyncExternalStore(
    (onChange) => useSettingsStore.persist.onFinishHydration(onChange),
    () => useSettingsStore.persist.hasHydrated(),
    () => true,
  );
  const [shortcut, setShortcut] = useState("CommandOrControl+Shift+V");
  const [enabled, setEnabled] = useState(true);
  const [block, setBlock] = useState<Block>("");
  const [notice, setNotice] = useState("");
  const desktop = isDesktop();

  useEffect(() => {
    if (!desktop || !hydrated || introduced) return;
    void clipboardGetSettings().then((settings) => {
      setShortcut(settings.shortcut);
      setEnabled(settings.enabled);
    }).catch(() => undefined);
  }, [desktop, hydrated, introduced]);

  function fail(next: Block, text: string) {
    setBlock(next);
    setNotice(text);
  }

  async function openClipboard() {
    setNotice("");
    setBlock("");
    try {
      if (!enabled) {
        await clipboardSetEnabled(true);
        setEnabled(true);
      }
      const permissions = await permissionsStatus();
      const access = permissions.find((item) => item.id === "accessibility");
      if (access && !access.granted) {
        fail("accessibility", t("zdx.text.intro.needAccessibility"));
        return;
      }
      await clipboardShowPanel();
      setIntroduced(true);
    } catch {
      fail("error", t("zdx.text.intro.actionFailed"));
    }
  }

  async function tryShot() {
    setNotice("");
    setBlock("");
    try {
      const current = await screenshotStatus();
      if (current.platform === "macos" && !current.screenRecording) {
        const again = await screenshotRequestAccess().catch(() => current);
        if (!again.screenRecording) {
          fail("screen", t("zdx.text.intro.needScreen"));
          return;
        }
      }
      await screenshotStart();
      setIntroduced(true);
    } catch (error) {
      if (error instanceof ScreenshotRequestError && (error.code === "permission_denied" || error.params.reason === "relaunch_required")) {
        fail("screen", screenshotErrorText(t, error));
        return;
      }
      fail("error", error instanceof ScreenshotRequestError ? screenshotErrorText(t, error) : t("zdx.text.intro.actionFailed"));
    }
  }

  return (
    <Modal
      open={desktop && hydrated && !introduced}
      title={t("zdx.label.intro.title")}
      footer={null}
      onCancel={() => setIntroduced(true)}
    >
      <Typography.Paragraph>{t("zdx.text.intro.body", { shortcut: formatShortcut(shortcut, isApplePlatform()) })}</Typography.Paragraph>
      {notice && <Alert className="mb-3" type={block === "error" ? "error" : "warning"} showIcon message={notice} />}
      <Flex gap={8} wrap>
        <Button type="primary" onClick={() => void openClipboard()}>{t("zdx.label.intro.openClipboard")}</Button>
        <Button onClick={() => void tryShot()}>{t("zdx.label.intro.tryShot")}</Button>
        {block === "accessibility" && <Button onClick={() => void permissionsOpen("accessibility")}>{t("zdx.label.permission.openSettings")}</Button>}
        {block === "screen" && <Button onClick={() => void openScreenRecordingSettings()}>{t("zdx.label.permission.openSettings")}</Button>}
        <Button type="text" onClick={() => setIntroduced(true)}>{t("zdx.label.intro.later")}</Button>
      </Flex>
    </Modal>
  );
}
