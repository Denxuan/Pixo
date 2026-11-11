import { openUrl } from "@tauri-apps/plugin-opener";

import { getAutostart, setAutostart, setBackground, setTrayIcon } from "@/api/system";
import { TRAY_ICONS, TrayMarkGlyph, type TrayIconId } from "@/components/icons/TrayMark";
import ClipboardSettingsPage from "@/pages/ClipboardSettings";
import { useSettingsStore, useTrayIcon } from "@/stores/settings";
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
    <Flex align="flex-start" justify="space-between" gap={20}>
      <div>
        <Typography.Text strong>{t("zdx.label.system.version")}</Typography.Text>
        <br />
        <Typography.Text type="secondary">{t("zdx.text.system.versionHelp")}</Typography.Text>
        {currentVersion && <><br /><Typography.Text>{t("zdx.text.system.versionCurrent", { version: currentVersion })}</Typography.Text></>}
        {update?.updateAvailable && <><br /><Typography.Text>{t("zdx.text.system.updateAvailable", { version: update.latest })}</Typography.Text></>}
        {update && !update.updateAvailable && <><br /><Typography.Text type="secondary">{t("zdx.text.system.upToDate")}</Typography.Text></>}
        {updateError && <><br /><Typography.Text type="secondary">{t("zdx.text.system.updateError")}</Typography.Text></>}
      </div>
      <Space>
        <Button onClick={() => void check()} loading={checking} disabled={!desktop}>{t("zdx.label.system.checkUpdate")}</Button>
        {update?.updateAvailable && <Button type="primary" onClick={() => void openUrl(update.url)}>{t("zdx.label.system.install")}</Button>}
      </Space>
    </Flex>
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
    <Flex vertical gap={8}>
      <div>
        <Typography.Text strong>{t("zdx.label.system.trayIcon")}</Typography.Text>
        <br />
        <Typography.Text type="secondary">{t("zdx.text.system.trayIcon")}</Typography.Text>
      </div>
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
                width: 76,
                height: 76,
                borderRadius: 12,
                border: `1px solid ${selected ? token.colorPrimary : token.colorBorderSecondary}`,
                background: selected ? token.colorPrimaryBg : "transparent",
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
    </Flex>
  );
}

export default function SystemSettingsPage() {
  const { t } = useTranslation();
  const background = useSettingsStore((state) => state.background);
  const saveBackground = useSettingsStore((state) => state.setBackground);
  const [autostart, setAutostartState] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const desktop = isDesktop();
  const section = location.pathname === "/settings/clipboard" ? "clipboard" : "general";

  useEffect(() => {
    if (!desktop) { setLoading(false); return; }
    let alive = true;
    void getAutostart().then((enabled) => { if (alive) setAutostartState(enabled); })
      .catch(() => { if (alive) setError(true); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [desktop]);

  async function update(kind: "autostart" | "background", enabled: boolean) {
    setBusy(true);
    setError(false);
    try {
      if (kind === "autostart") { await setAutostart(enabled); setAutostartState(await getAutostart()); }
      else { await setBackground(enabled); saveBackground(enabled); }
    } catch { setError(true); }
    finally { setBusy(false); }
  }

  return <Flex vertical gap={20} className="min-h-0 flex-1 overflow-auto">
    <Typography.Title level={3} style={{ margin: 0 }}>{t("zdx.label.system.title")}</Typography.Title>
    <Tabs
      activeKey={section}
      onChange={(key) => navigate(key === "clipboard" ? "/settings/clipboard" : "/settings")}
      items={[
        {
          key: "general",
          label: t("zdx.label.system.general"),
          children: (
            <Flex vertical gap={24} className="max-w-xl">
              {!desktop && <Alert type="info" showIcon message={t("zdx.text.system.desktopOnly")} />}
              {error && <Alert type="error" showIcon message={t("zdx.text.system.error")} />}
              <TrayIconPicker />
              <Flex align="center" justify="space-between" gap={20}><div><Typography.Text strong>{t("zdx.label.system.autostart")}</Typography.Text><br /><Typography.Text type="secondary">{t("zdx.text.system.autostart")}</Typography.Text></div><Switch aria-label={t("zdx.label.system.autostart")} checked={autostart} disabled={!desktop || loading || busy} onChange={(enabled) => void update("autostart", enabled)} /></Flex>
              <Flex align="center" justify="space-between" gap={20}><div><Typography.Text strong>{t("zdx.label.system.background")}</Typography.Text><br /><Typography.Text type="secondary">{t("zdx.text.system.background")}</Typography.Text></div><Switch aria-label={t("zdx.label.system.background")} checked={background} disabled={!desktop || busy} onChange={(enabled) => void update("background", enabled)} /></Flex>
              <VersionCheck desktop={desktop} />
            </Flex>
          ),
        },
        {
          key: "clipboard",
          label: t("zdx.label.system.clipboard"),
          children: <ClipboardSettingsPage />,
        },
      ]}
    />
  </Flex>;
}
