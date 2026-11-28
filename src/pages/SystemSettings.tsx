import { openUrl } from "@tauri-apps/plugin-opener";

import { clipboardSetEnabled, clipboardStatus } from "@/api/clipboard";
import { screenshotGetSettings, screenshotSetEnabled, screenshotStatus } from "@/api/screenshot";
import { dockerSetEnabled, dockerStatus } from "@/api/docker";
import { sshSetEnabled, sshStatus } from "@/api/ssh";
import { getAutostart, setAutostart, setBackground, setLauncherShortcut, setTrayIcon } from "@/api/system";
import ShortcutField from "@/components/clipboard/ShortcutField";
import type { ClipboardStatus } from "@/types/clipboard";
import type { ShotStatus } from "@/types/screenshot";
import type { SshStatus } from "@/types/ssh";
import { clipboardReasonText } from "@/utils/clipboardMessage";
import { screenshotReasonText } from "@/utils/screenshotMessage";
import { isApplePlatform } from "@/utils/shortcut";
import { inkFrameStyle } from "@/utils/theme";
import { TRAY_ICONS, TrayMarkGlyph, type TrayIconId } from "@/components/icons/TrayMark";
import Page from "@/components/Page";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import ClipboardSettingsPage from "@/pages/ClipboardSettings";
import DataSettingsPage from "@/pages/DataSettings";
import DockerSettingsPage from "@/pages/DockerSettings";
import ScreenshotSettingsPage from "@/pages/ScreenshotSettings";
import SshSettingsPage from "@/pages/SshSettings";
import { useSettingsStore, useTrayIcon, type Language, type ThemeMode } from "@/stores/settings";
import { useUpdateStore } from "@/stores/update";
import { isDesktop } from "@/utils/desktop";

function VersionCheck({ desktop }: { desktop: boolean }) {
  const { t } = useTranslation();
  const currentVersion = useUpdateStore((state) => state.currentVersion);
  const update = useUpdateStore((state) => state.update);
  const checking = useUpdateStore((state) => state.checking);
  const updateError = useUpdateStore((state) => state.error);
  const check = useUpdateStore((state) => state.check);

  return (
    <SettingRow
      label={currentVersion ? t("zdx.text.system.versionCurrent", { version: currentVersion }) : t("zdx.label.system.version")}
      description={
        <>
          <div>{t("zdx.text.system.versionHelp")}</div>
          {update?.updateAvailable && <Typography.Text type="success" style={{ fontSize: "inherit" }}>{t("zdx.text.system.updateAvailable", { version: update.latest })}</Typography.Text>}
          {update && !update.updateAvailable && <div>{t("zdx.text.system.upToDate")}</div>}
          {updateError && <Typography.Text type="warning" style={{ fontSize: "inherit" }}>{t("zdx.text.system.updateError")}</Typography.Text>}
        </>
      }
    >
      <Space>
        <Button onClick={() => void check()} loading={checking} disabled={!desktop}>{t("zdx.label.system.checkUpdate")}</Button>
        {update?.updateAvailable && <Button type="primary" onClick={() => void openUrl(update.url)}>{t("zdx.label.system.install")}</Button>}
      </Space>
    </SettingRow>
  );
}

function LauncherShortcut() {
  const { t } = useTranslation();
  const shortcut = useSettingsStore((state) => state.launcherShortcut);
  const save = useSettingsStore((state) => state.setLauncherShortcut);
  const [error, setError] = useState("");
  const desktop = isDesktop();

  async function change(next: string) {
    const previous = shortcut;
    save(next);
    if (!desktop) return;
    setError("");
    try {
      await setLauncherShortcut(next);
    } catch {
      save(previous);
      setError(t("zdx.text.system.launcherFailed"));
    }
  }

  return (
    <SettingGroup title={t("zdx.label.launcher.title")}>
      <SettingRow label={t("zdx.label.system.launcher")} description={error || t("zdx.text.system.launcher")}>
        <ShortcutField value={shortcut} macos={isApplePlatform()} label={t("zdx.label.system.launcher")} onChange={(next) => void change(next)} />
      </SettingRow>
    </SettingGroup>
  );
}

function SessionCapabilities({ desktop }: { desktop: boolean }) {
  const { t } = useTranslation();
  const [clipboard, setClipboard] = useState<ClipboardStatus | null>(null);
  const [screenshot, setScreenshot] = useState<ShotStatus | null>(null);
  const [ssh, setSsh] = useState<SshStatus | null>(null);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void clipboardStatus().then((current) => { if (alive) setClipboard(current); }).catch(() => undefined);
    void screenshotStatus().then((current) => { if (alive) setScreenshot(current); }).catch(() => undefined);
    void sshStatus().then((current) => { if (alive) setSsh(current); }).catch(() => undefined);
    return () => { alive = false; };
  }, [desktop]);

  const pending = t("zdx.text.system.sessionPending");
  const clipboardLines = clipboard
    ? [clipboard.canMonitor ? t("zdx.text.system.canListen") : t("zdx.text.system.cannotListen"), clipboard.canGlobalShortcut ? t("zdx.text.system.canShortcut") : t("zdx.text.system.cannotShortcut"), clipboardReasonText(t, clipboard.reason)].filter(Boolean)
    : [pending];
  const screenshotLines = screenshot
    ? [screenshot.canCapture ? t("zdx.text.system.canCapture") : t("zdx.text.system.cannotCapture"), screenshot.canOcr ? t("zdx.text.system.canOcr") : t("zdx.text.system.cannotOcr"), screenshot.canRecord ? t("zdx.text.system.canRecord") : t("zdx.text.system.cannotRecord"), screenshotReasonText(t, screenshot.captureReason)].filter(Boolean)
    : [pending];
  const sshLines = ssh
    ? [ssh.keyringAvailable ? t("zdx.text.system.keyringOn") : t("zdx.text.system.keyringOff"), ssh.agentAvailable ? t("zdx.text.system.agentOn") : t("zdx.text.system.agentOff")]
    : [pending];

  return (
    <SettingGroup title={t("zdx.label.system.session")}>
      <SettingRow label={t("zdx.label.system.clipboardFeature")} description={<CapabilityLines lines={desktop ? clipboardLines : [t("zdx.text.system.desktopOnly")]} />} />
      <SettingRow label={t("zdx.label.system.screenshotFeature")} description={<CapabilityLines lines={desktop ? screenshotLines : [t("zdx.text.system.desktopOnly")]} />} />
      <SettingRow label={t("zdx.label.system.sshFeature")} description={<CapabilityLines lines={desktop ? sshLines : [t("zdx.text.system.desktopOnly")]} />} />
    </SettingGroup>
  );
}

function CapabilityLines({ lines }: { lines: string[] }) {
  return (
    <>
      {lines.map((line) => <div key={line}>{line}</div>)}
    </>
  );
}

function isThemeMode(value: string | number): value is ThemeMode {
  return value === "system" || value === "light" || value === "dark";
}

function isLanguage(value: string): value is Language {
  return value === "zh-CN" || value === "en";
}

function AppearanceSettings() {
  const { t } = useTranslation();
  const mode = useSettingsStore((state) => state.mode);
  const setMode = useSettingsStore((state) => state.setMode);
  const language = useSettingsStore((state) => state.language);
  const setLanguage = useSettingsStore((state) => state.setLanguage);

  return (
    <SettingGroup title={t("zdx.label.system.appearance")}>
      <SettingRow label={t("zdx.label.theme.title")} description={t("zdx.text.system.theme")}>
        <Segmented
          aria-label={t("zdx.label.theme.title")}
          value={mode}
          onChange={(value) => {
            if (isThemeMode(value)) setMode(value);
          }}
          options={[
            { value: "system", label: t("zdx.label.theme.system") },
            { value: "light", label: t("zdx.label.theme.light") },
            { value: "dark", label: t("zdx.label.theme.dark") },
          ]}
        />
      </SettingRow>
      <SettingRow label={t("zdx.label.language.title")} description={t("zdx.text.system.language")}>
        <Select
          aria-label={t("zdx.label.language.title")}
          value={language}
          style={{ width: 160 }}
          onChange={(value) => {
            if (isLanguage(value)) setLanguage(value);
          }}
          options={[
            { value: "zh-CN", label: t("zdx.label.language.chinese") },
            { value: "en", label: t("zdx.label.language.english") },
          ]}
        />
      </SettingRow>
    </SettingGroup>
  );
}

function TrayIconPicker() {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const trayIcon = useTrayIcon();
  const saveTrayIcon = useSettingsStore((state) => state.setTrayIcon);
  const [failed, setFailed] = useState(false);
  const desktop = isDesktop();

  function choose(id: TrayIconId) {
    saveTrayIcon(id);
    if (!desktop) return;
    setFailed(false);
    void setTrayIcon(id).catch(() => setFailed(true));
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const current = TRAY_ICONS.findIndex((icon) => icon.id === trayIcon);
    const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
    const backward = event.key === "ArrowLeft" || event.key === "ArrowUp";
    if (!forward && !backward) return;
    event.preventDefault();
    const next = TRAY_ICONS[(current + (forward ? 1 : -1) + TRAY_ICONS.length) % TRAY_ICONS.length];
    choose(next.id);
    event.currentTarget.querySelector<HTMLButtonElement>(`[data-tray-icon="${next.id}"]`)?.focus();
  }

  return (
    <SettingRow stacked label={t("zdx.label.system.trayIcon")} description={t("zdx.text.system.trayIcon")}>
      <div role="radiogroup" aria-label={t("zdx.label.system.trayIcon")} className="flex flex-wrap gap-2" onKeyDown={onKeyDown}>
        {TRAY_ICONS.map((icon) => {
          const selected = trayIcon === icon.id;
          return (
            <button
              key={icon.id}
              type="button"
              role="radio"
              data-tray-icon={icon.id}
              aria-checked={selected}
              aria-label={t(icon.label)}
              tabIndex={selected ? 0 : -1}
              onClick={() => choose(icon.id)}
              className="flex cursor-pointer flex-col items-center justify-center gap-1 bg-transparent"
              style={{
                minWidth: 72,
                height: 68,
                padding: "0 8px",
                background: selected ? token.colorPrimaryBg : token.colorFillQuaternary,
                color: selected ? token.colorPrimary : token.colorText,
                ...inkFrameStyle(token, token.borderRadius, 2),
                borderColor: selected ? token.colorPrimary : token.colorBorder,
              }}
            >
              <TrayMarkGlyph id={icon.id} />
              <span style={{ fontSize: 12 }}>{t(icon.label)}</span>
            </button>
          );
        })}
      </div>
      {failed && <Typography.Text type="danger">{t("zdx.text.system.error")}</Typography.Text>}
    </SettingRow>
  );
}

export default function SystemSettingsPage() {
  const { t } = useTranslation();
  const background = useSettingsStore((state) => state.background);
  const saveBackground = useSettingsStore((state) => state.setBackground);
  const [autostart, setAutostartState] = useState(false);
  const [clipboardOn, setClipboardOn] = useState<boolean | null>(null);
  const [screenshotOn, setScreenshotOn] = useState<boolean | null>(null);
  const [sshOn, setSshOn] = useState<boolean | null>(null);
  const [dockerOn, setDockerOn] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const desktop = isDesktop();
  const showClipboardSettings = !desktop || clipboardOn !== false;
  const showScreenshotSettings = !desktop || screenshotOn !== false;
  const showSshSettings = !desktop || sshOn !== false;
  const showDockerSettings = !desktop || dockerOn !== false;
  const section = location.pathname === "/settings/clipboard" && showClipboardSettings
    ? "clipboard"
    : location.pathname === "/settings/screenshot" && showScreenshotSettings
      ? "screenshot"
      : location.pathname === "/settings/ssh" && showSshSettings
        ? "ssh"
        : location.pathname === "/settings/docker" && showDockerSettings
          ? "docker"
          : location.pathname === "/settings/data"
            ? "data"
            : "general";

  useEffect(() => {
    if (!desktop) { setLoading(false); return; }
    let alive = true;
    void Promise.all([getAutostart(), clipboardStatus(), screenshotGetSettings(), sshStatus(), dockerStatus()]).then(([enabled, clipboard, screenshot, ssh, docker]) => {
      if (!alive) return;
      setAutostartState(enabled);
      setClipboardOn(clipboard.enabled);
      setScreenshotOn(screenshot.enabled);
      setSshOn(ssh.enabled);
      setDockerOn(docker.enabled);
    }).catch(() => { if (alive) setError(true); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [desktop]);

  useEffect(() => {
    if (!desktop || loading) return;
    const blocked =
      (location.pathname === "/settings/clipboard" && clipboardOn === false)
      || (location.pathname === "/settings/screenshot" && screenshotOn === false)
      || (location.pathname === "/settings/ssh" && sshOn === false)
      || (location.pathname === "/settings/docker" && dockerOn === false);
    if (blocked) navigate("/settings", { replace: true });
  }, [clipboardOn, desktop, dockerOn, loading, location.pathname, navigate, screenshotOn, sshOn]);

  async function update(kind: "autostart" | "background" | "clipboard" | "screenshot" | "ssh" | "docker", enabled: boolean) {
    setBusy(true);
    setError(false);
    try {
      if (kind === "autostart") { await setAutostart(enabled); setAutostartState(await getAutostart()); }
      else if (kind === "background") { await setBackground(enabled); saveBackground(enabled); }
      else if (kind === "clipboard") { setClipboardOn((await clipboardSetEnabled(enabled)).enabled); }
      else if (kind === "screenshot") {
        const next = await screenshotSetEnabled(enabled);
        setScreenshotOn(next.enabled);
        if (next.enabled === false && location.pathname === "/settings/screenshot") navigate("/settings", { replace: true });
      } else if (kind === "ssh") {
        setSshOn((await sshSetEnabled(enabled)).enabled);
      } else {
        setDockerOn((await dockerSetEnabled(enabled)).enabled);
      }
    } catch { setError(true); }
    finally { setBusy(false); }
  }

  function changeSsh(enabled: boolean) {
    Modal.confirm({
      title: t(enabled ? "zdx.text.ssh.enableTitle" : "zdx.text.ssh.disableTitle"),
      content: t(enabled ? "zdx.text.ssh.enableBody" : "zdx.text.ssh.disableBody"),
      okText: t(enabled ? "zdx.label.ssh.enable" : "zdx.label.ssh.confirm"),
      cancelText: t("zdx.label.ssh.cancel"),
      onOk: () => update("ssh", enabled),
    });
  }

  function changeDocker(enabled: boolean) {
    Modal.confirm({
      title: t(enabled ? "zdx.text.docker.enableTitle" : "zdx.text.docker.disableTitle"),
      content: t(enabled ? "zdx.text.docker.enableBody" : "zdx.text.docker.disableBody"),
      okText: t(enabled ? "zdx.label.docker.enable" : "zdx.label.docker.confirm"),
      cancelText: t("zdx.label.docker.cancel"),
      onOk: () => update("docker", enabled),
    });
  }

  function changeClipboard(enabled: boolean) {
    if (!enabled) {
      void update("clipboard", false);
      return;
    }
    Modal.confirm({
      title: t("zdx.text.clipboard.enableTitle"),
      content: t("zdx.text.clipboard.enableBody"),
      okText: t("zdx.label.clipboard.enable"),
      cancelText: t("zdx.label.clipboard.cancel"),
      onOk: () => update("clipboard", true),
    });
  }

  return <Page title={t("zdx.label.system.title")}>
    <Tabs
      activeKey={section}
      onChange={(key) => navigate(key === "general" ? "/settings" : `/settings/${key}`)}
      destroyOnHidden
      items={[
        {
          key: "general",
          label: t("zdx.label.system.general"),
          children: (
            <Flex vertical gap={24}>
              {!desktop && <Alert type="info" showIcon message={t("zdx.text.system.desktopOnly")} />}
              {error && <Alert type="error" showIcon message={t("zdx.text.system.error")} />}
              <AppearanceSettings />
              <SettingGroup title={t("zdx.label.system.features")}>
                <SettingRow label={t("zdx.label.system.clipboardFeature")} description={t("zdx.text.system.clipboardFeature")}>
                  <Switch aria-label={t("zdx.label.system.clipboardFeature")} checked={clipboardOn === true} disabled={!desktop || clipboardOn === null || busy} onChange={changeClipboard} />
                </SettingRow>
                <SettingRow label={t("zdx.label.system.screenshotFeature")} description={t("zdx.text.system.screenshotFeature")}>
                  <Switch aria-label={t("zdx.label.system.screenshotFeature")} checked={screenshotOn === true} disabled={!desktop || screenshotOn === null || busy} onChange={(enabled) => void update("screenshot", enabled)} />
                </SettingRow>
                <SettingRow label={t("zdx.label.system.sshFeature")} description={t("zdx.text.system.sshFeature")}>
                  <Switch aria-label={t("zdx.label.system.sshFeature")} checked={sshOn === true} disabled={!desktop || sshOn === null || busy} onChange={changeSsh} />
                </SettingRow>
                <SettingRow label={t("zdx.label.system.dockerFeature")} description={t("zdx.text.system.dockerFeature")}>
                  <Switch aria-label={t("zdx.label.system.dockerFeature")} checked={dockerOn === true} disabled={!desktop || dockerOn === null || busy} onChange={changeDocker} />
                </SettingRow>
              </SettingGroup>
              <SettingGroup title={t("zdx.label.system.startup")}>
                <SettingRow label={t("zdx.label.system.autostart")} description={t("zdx.text.system.autostart")}>
                  <Switch aria-label={t("zdx.label.system.autostart")} checked={autostart} disabled={!desktop || loading || busy} onChange={(enabled) => void update("autostart", enabled)} />
                </SettingRow>
                <SettingRow label={t("zdx.label.system.background")} description={t("zdx.text.system.background")}>
                  <Switch aria-label={t("zdx.label.system.background")} checked={background} disabled={!desktop || busy} onChange={(enabled) => void update("background", enabled)} />
                </SettingRow>
                <TrayIconPicker />
              </SettingGroup>
              <LauncherShortcut />
              <SessionCapabilities desktop={desktop} />
              <SettingGroup title={t("zdx.label.system.about")}>
                <VersionCheck desktop={desktop} />
              </SettingGroup>
            </Flex>
          ),
        },
        {
          key: "data",
          label: t("zdx.label.system.data"),
          children: <DataSettingsPage />,
        },
        ...(showClipboardSettings ? [{
          key: "clipboard",
          label: t("zdx.label.system.clipboard"),
          children: <ClipboardSettingsPage />,
        }] : []),
        ...(showScreenshotSettings ? [{
          key: "screenshot",
          label: t("zdx.label.system.screenshot"),
          children: <ScreenshotSettingsPage />,
        }] : []),
        ...(showSshSettings ? [{
          key: "ssh",
          label: t("zdx.label.system.ssh"),
          children: <SshSettingsPage />,
        }] : []),
        ...(showDockerSettings ? [{
          key: "docker",
          label: t("zdx.label.system.docker"),
          children: <DockerSettingsPage />,
        }] : []),
      ]}
    />
  </Page>;
}
