import { clipboardSetEnabled, clipboardStatus } from "@/api/clipboard";
import { dockerSetEnabled, dockerStatus } from "@/api/docker";
import { permissionsOpen, permissionsPrepare, permissionsStatus, type SystemPermission } from "@/api/permissions";
import { ScreenshotRequestError, openScreenRecordingSettings, screenshotGetSettings, screenshotRequestAccess, screenshotSetEnabled, screenshotStatus } from "@/api/screenshot";
import { sshSetEnabled, sshStatus } from "@/api/ssh";
import { useSettingsStore } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";
import { holdPermissionPrompt } from "@/utils/permissionGate";
import { screenshotErrorText } from "@/utils/screenshotMessage";
import { formatShortcut, isApplePlatform } from "@/utils/shortcut";

type ToolId = "clipboard" | "screenshot" | "ssh" | "docker";
type Step = "tools" | "access" | "ready";

const TOOLS: ToolId[] = ["clipboard", "screenshot", "ssh", "docker"];

export default function FirstRun() {
  const { t } = useTranslation();
  const introduced = useSettingsStore((state) => state.introduced);
  const setIntroduced = useSettingsStore((state) => state.setIntroduced);
  const shortcut = useSettingsStore((state) => state.launcherShortcut);
  const hydrated = useSyncExternalStore(
    (onChange) => useSettingsStore.persist.onFinishHydration(onChange),
    () => useSettingsStore.persist.hasHydrated(),
    () => true,
  );
  const [step, setStep] = useState<Step>("tools");
  const [picked, setPicked] = useState<Record<ToolId, boolean>>({ clipboard: false, screenshot: false, ssh: false, docker: false });
  const [known, setKnown] = useState(false);
  const [missing, setMissing] = useState<SystemPermission["id"][]>([]);
  const [screenMissing, setScreenMissing] = useState(false);
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const desktop = isDesktop();
  const open = desktop && hydrated && !introduced;

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setKnown(false);
    void Promise.all([clipboardStatus(), screenshotGetSettings(), sshStatus(), dockerStatus()])
      .then(([clipboard, screenshot, ssh, docker]) => {
        if (!alive) return;
        setPicked({
          clipboard: clipboard.enabled,
          screenshot: screenshot.enabled,
          ssh: ssh.enabled,
          docker: docker.enabled,
        });
        setKnown(true);
      })
      .catch(() => {
        if (alive) setNotice(t("zdx.text.intro.actionFailed"));
      });
    return () => {
      alive = false;
    };
  }, [open]);

  function finish() {
    holdPermissionPrompt();
    setIntroduced(true);
  }

  function toggle(id: ToolId, checked: boolean) {
    setPicked((current) => ({ ...current, [id]: checked }));
  }

  async function collectAccess() {
    const next: SystemPermission["id"][] = [];
    let screen = false;
    if (picked.clipboard) {
      await permissionsPrepare().catch(() => undefined);
      const permissions = await permissionsStatus();
      for (const item of permissions) {
        if (!item.granted) next.push(item.id);
      }
    }
    if (picked.screenshot) {
      const current = await screenshotStatus();
      if (current.platform === "macos" && !current.screenRecording) {
        const again = await screenshotRequestAccess().catch(() => current);
        screen = !again.screenRecording;
      }
    }
    setMissing(next);
    setScreenMissing(screen);
    return next.length > 0 || screen;
  }

  async function applyTools() {
    setBusy(true);
    setNotice("");
    try {
      await clipboardSetEnabled(picked.clipboard);
      await screenshotSetEnabled(picked.screenshot);
      await sshSetEnabled(picked.ssh);
      await dockerSetEnabled(picked.docker);
      const blocked = await collectAccess();
      setStep(blocked ? "access" : "ready");
    } catch (error) {
      setNotice(error instanceof ScreenshotRequestError ? screenshotErrorText(t, error) : t("zdx.text.intro.actionFailed"));
    } finally {
      setBusy(false);
    }
  }

  async function recheck() {
    setBusy(true);
    setNotice("");
    try {
      const blocked = await collectAccess();
      if (!blocked) setStep("ready");
    } catch (error) {
      setNotice(error instanceof ScreenshotRequestError ? screenshotErrorText(t, error) : t("zdx.text.intro.actionFailed"));
    } finally {
      setBusy(false);
    }
  }

  const title = step === "tools" ? t("zdx.label.intro.tools") : step === "access" ? t("zdx.label.intro.access") : t("zdx.label.intro.ready");

  return (
    <Modal open={open} title={title} footer={null} onCancel={finish} maskClosable={false}>
      {notice && <Alert className="mb-3" type="warning" showIcon message={notice} />}
      {step === "tools" && (
        <Flex vertical gap={12}>
          <Typography.Paragraph style={{ margin: 0 }}>{t("zdx.text.intro.tools")}</Typography.Paragraph>
          {TOOLS.map((id) => (
            <Checkbox key={id} checked={picked[id]} disabled={!known || busy} onChange={(event) => toggle(id, event.target.checked)}>
              <span className="inline-flex flex-col">
                <span>{t(`zdx.label.intro.tool.${id}`)}</span>
                <Typography.Text type="secondary">{t(`zdx.text.intro.tool.${id}`)}</Typography.Text>
              </span>
            </Checkbox>
          ))}
          <Flex gap={8} wrap>
            <Button type="primary" loading={busy} disabled={!known} onClick={() => void applyTools()}>{t("zdx.label.intro.continue")}</Button>
            <Button type="text" onClick={finish}>{t("zdx.label.intro.later")}</Button>
          </Flex>
        </Flex>
      )}
      {step === "access" && (
        <Flex vertical gap={12}>
          <Typography.Paragraph style={{ margin: 0 }}>{t("zdx.text.intro.access")}</Typography.Paragraph>
          {missing.map((id) => (
            <Flex key={id} justify="space-between" align="center" gap={12}>
              <span>{t(`zdx.label.permission.${id}`)}</span>
              <Button onClick={() => void permissionsOpen(id)}>{t("zdx.label.permission.openSettings")}</Button>
            </Flex>
          ))}
          {screenMissing && (
            <Flex justify="space-between" align="center" gap={12}>
              <span>{t("zdx.text.intro.needScreen")}</span>
              <Button onClick={() => void openScreenRecordingSettings()}>{t("zdx.label.permission.openSettings")}</Button>
            </Flex>
          )}
          <Flex gap={8} wrap>
            <Button type="primary" loading={busy} onClick={() => void recheck()}>{t("zdx.label.intro.recheck")}</Button>
            <Button onClick={() => setStep("ready")}>{t("zdx.label.intro.continue")}</Button>
          </Flex>
        </Flex>
      )}
      {step === "ready" && (
        <Flex vertical gap={12}>
          <Typography.Paragraph style={{ margin: 0 }}>{t("zdx.text.intro.ready", { shortcut: formatShortcut(shortcut, isApplePlatform()) })}</Typography.Paragraph>
          <Button type="primary" onClick={finish}>{t("zdx.label.intro.finish")}</Button>
        </Flex>
      )}
    </Modal>
  );
}
