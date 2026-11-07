import { useSettingsStore, type ThemeMode } from "@/stores/settings";

function isThemeMode(value: string): value is ThemeMode {
  return value === "system" || value === "light" || value === "dark";
}

export default function AppShell() {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const mode = useSettingsStore((state) => state.mode);
  const setMode = useSettingsStore((state) => state.setMode);
  const language = useSettingsStore((state) => state.language);
  const setLanguage = useSettingsStore((state) => state.setLanguage);
  const selected = location.pathname.startsWith("/clipboard") ? "clipboard" : "";

  return (
    <Layout className="h-full overflow-hidden">
      <Layout.Sider width={232} theme="dark">
        <Flex vertical style={{ height: "100%" }}>
          <Flex align="center" gap={12} style={{ padding: "24px 20px 8px" }}>
            <img src="/pixo-icon.png" alt="" className="h-10 w-10 rounded-xl" />
            <Flex vertical gap={0}>
              <Typography.Title level={4} style={{ margin: 0, color: "#fff" }}>Pixo</Typography.Title>
              <Typography.Text style={{ color: "rgba(255,255,255,0.65)" }}>{t("zdx.text.app.tagline")}</Typography.Text>
            </Flex>
          </Flex>
          <Menu
            theme="dark"
            mode="inline"
            selectedKeys={selected ? [selected] : []}
            style={{ borderInlineEnd: 0 }}
            onClick={({ key }) => {
              if (key === "clipboard") navigate("/clipboard");
            }}
            items={[
              {
                type: "group",
                label: t("zdx.label.nav.workspace"),
                children: [
                  { key: "clipboard", icon: <HistoryOutlined />, label: t("zdx.label.nav.clipboard") },
                ],
              },
            ]}
          />
          <Typography.Text style={{ marginTop: "auto", padding: 20, color: "rgba(255,255,255,0.65)" }}>{t("zdx.text.app.local")}</Typography.Text>
        </Flex>
      </Layout.Sider>
      <Layout className="min-h-0 overflow-hidden">
        <Layout.Header style={{ display: "flex", alignItems: "center", justifyContent: "space-between", paddingInline: 24 }}>
          <Typography.Text type="secondary">{t("zdx.text.app.welcome")}</Typography.Text>
          <Space>
            <Select
              aria-label={t("zdx.label.theme.title")}
              value={mode}
              style={{ width: 128 }}
              onChange={(value) => {
                if (isThemeMode(value)) setMode(value);
              }}
              options={[
                { value: "system", label: t("zdx.label.theme.system") },
                { value: "light", label: t("zdx.label.theme.light") },
                { value: "dark", label: t("zdx.label.theme.dark") },
              ]}
            />
            <Button icon={<GlobalOutlined />} aria-label={t("zdx.label.language.switch")} onClick={() => setLanguage(language === "en" ? "zh-CN" : "en")}>
              {t("zdx.label.language.toggle")}
            </Button>
          </Space>
        </Layout.Header>
        <Layout.Content className="flex min-h-0 flex-1 flex-col overflow-hidden" style={{ padding: 24 }}>
          <Outlet />
        </Layout.Content>
      </Layout>
    </Layout>
  );
}
