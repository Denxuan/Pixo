import { openUrl } from "@tauri-apps/plugin-opener";

import { clipboardSetEnabled, clipboardStatus } from "@/api/clipboard";
import { screenshotGetSettings, screenshotSetEnabled } from "@/api/screenshot";
import { getAutostart, setAutostart, setBackground, setTrayIcon } from "@/api/system";
import { TRAY_ICONS, TrayMarkGlyph, type TrayIconId } from "@/components/icons/TrayMark";
import Page from "@/components/Page";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import ClipboardSettingsPage from "@/pages/ClipboardSettings";
import ScreenshotSettingsPage from "@/pages/ScreenshotSettings";
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
                borderRadius: token.borderRadius,
                border: `1px solid ${selected ? token.colorPrimary : token.colorBorderSecondary}`,
                background: selected ? token.colorPrimaryBg : token.colorFillQuaternary,
                color: selected ? token.colorPrimary : token.colorText,
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
  const [clipboardOn, setClipboardOn] = useState(false);
  const [screenshotOn, setScreenshotOn] = useState(true);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const desktop = isDesktop();
  const screenshotSettingsOpen = screenshotOn && location.pathname === "/settings/screenshot";
  const section = location.pathname === "/settings/clipboard" ? "clipboard" : screenshotSettingsOpen ? "screenshot" : "general";

  useEffect(() => {
    if (!desktop) { setLoading(false); return; }
    let alive = true;
    void Promise.all([getAutostart(), clipboardStatus(), screenshotGetSettings()]).then(([enabled, clipboard, screenshot]) => {
      if (!alive) return;
      setAutostartState(enabled);
      setClipboardOn(clipboard.enabled);
      setScreenshotOn(screenshot.enabled);
    }).catch(() => { if (alive) setError(true); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [desktop]);

  useEffect(() => {
    if (!desktop || loading || screenshotOn || location.pathname !== "/settings/screenshot") return;
    navigate("/settings", { replace: true });
  }, [desktop, loading, location.pathname, navigate, screenshotOn]);

  async function update(kind: "autostart" | "background" | "clipboard" | "screenshot", enabled: boolean) {
    setBusy(true);
    setError(false);
    try {
      if (kind === "autostart") { await setAutostart(enabled); setAutostartState(await getAutostart()); }
      else if (kind === "background") { await setBackground(enabled); saveBackground(enabled); }
      else if (kind === "clipboard") { setClipboardOn((await clipboardSetEnabled(enabled)).enabled); }
      else {
        const next = await screenshotSetEnabled(enabled);
        setScreenshotOn(next.enabled);
        if (!next.enabled && location.pathname === "/settings/screenshot") navigate("/settings", { replace: true });
      }
    } catch { setError(true); }
    finally { setBusy(false); }
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
      onChange={(key) => navigate(key === "clipboard" ? "/settings/clipboard" : key === "screenshot" ? "/settings/screenshot" : "/settings")}
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
                  <Switch aria-label={t("zdx.label.system.clipboardFeature")} checked={clipboardOn} disabled={!desktop || loading || busy} onChange={changeClipboard} />
                </SettingRow>
                <SettingRow label={t("zdx.label.system.screenshotFeature")} description={t("zdx.text.system.screenshotFeature")}>
                  <Switch aria-label={t("zdx.label.system.screenshotFeature")} checked={screenshotOn} disabled={!desktop || loading || busy} onChange={(enabled) => void update("screenshot", enabled)} />
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
              <SettingGroup title={t("zdx.label.system.about")}>
                <VersionCheck desktop={desktop} />
              </SettingGroup>
            </Flex>
          ),
        },
        {
          key: "clipboard",
          label: t("zdx.label.system.clipboard"),
          children: <ClipboardSettingsPage />,
        },
        ...(screenshotOn ? [{
          key: "screenshot",
          label: t("zdx.label.system.screenshot"),
          children: <ScreenshotSettingsPage />,
        }] : []),
      ]}
    />
  </Page>;
}
