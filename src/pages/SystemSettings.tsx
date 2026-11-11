import { getAutostart, setAutostart, setBackground } from "@/api/system";
import ClipboardSettingsPage from "@/pages/ClipboardSettings";
import { useSettingsStore } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";

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
    <div><Typography.Title level={3}>{t("zdx.label.system.title")}</Typography.Title><Typography.Paragraph type="secondary">{t("zdx.text.system.description")}</Typography.Paragraph></div>
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
              <Flex align="center" justify="space-between" gap={20}><div><Typography.Text strong>{t("zdx.label.system.autostart")}</Typography.Text><br /><Typography.Text type="secondary">{t("zdx.text.system.autostart")}</Typography.Text></div><Switch aria-label={t("zdx.label.system.autostart")} checked={autostart} disabled={!desktop || loading || busy} onChange={(enabled) => void update("autostart", enabled)} /></Flex>
              <Flex align="center" justify="space-between" gap={20}><div><Typography.Text strong>{t("zdx.label.system.background")}</Typography.Text><br /><Typography.Text type="secondary">{t("zdx.text.system.background")}</Typography.Text></div><Switch aria-label={t("zdx.label.system.background")} checked={background} disabled={!desktop || busy} onChange={(enabled) => void update("background", enabled)} /></Flex>
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
