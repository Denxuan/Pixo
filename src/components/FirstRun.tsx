import { clipboardGetSettings, clipboardSetEnabled, clipboardShowPanel } from "@/api/clipboard";
import { screenshotStart } from "@/api/screenshot";
import { useSettingsStore } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";
import { formatShortcut, isApplePlatform } from "@/utils/shortcut";

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
  const desktop = isDesktop();

  useEffect(() => {
    if (!desktop || !hydrated || introduced) return;
    void clipboardGetSettings().then((settings) => {
      setShortcut(settings.shortcut);
      setEnabled(settings.enabled);
    }).catch(() => undefined);
  }, [desktop, hydrated, introduced]);

  async function openClipboard() {
    if (!enabled) await clipboardSetEnabled(true).catch(() => undefined);
    await clipboardShowPanel().catch(() => undefined);
    setIntroduced(true);
  }

  async function tryShot() {
    await screenshotStart().catch(() => undefined);
    setIntroduced(true);
  }

  return (
    <Modal
      open={desktop && hydrated && !introduced}
      title={t("zdx.label.intro.title")}
      footer={null}
      onCancel={() => setIntroduced(true)}
    >
      <Typography.Paragraph>{t("zdx.text.intro.body", { shortcut: formatShortcut(shortcut, isApplePlatform()) })}</Typography.Paragraph>
      <Flex gap={8} wrap>
        <Button type="primary" onClick={() => void openClipboard()}>{t("zdx.label.intro.openClipboard")}</Button>
        <Button onClick={() => void tryShot()}>{t("zdx.label.intro.tryShot")}</Button>
        <Button type="text" onClick={() => setIntroduced(true)}>{t("zdx.label.intro.later")}</Button>
      </Flex>
    </Modal>
  );
}
