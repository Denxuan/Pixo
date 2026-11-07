import { listen } from "@tauri-apps/api/event";

import { takeClipboardMainRequest } from "@/api/clipboard";
import PermissionPrompt from "@/components/PermissionPrompt";
import { useSettingsStore, type ThemeMode } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";

const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
const subscribeMotion = (onChange: () => void) => {
  motionQuery.addEventListener("change", onChange);
  return () => motionQuery.removeEventListener("change", onChange);
};

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
  const { token } = antdTheme.useToken();
  const [collapsed, setCollapsed] = useState(false);
  const reduceMotion = useSyncExternalStore(subscribeMotion, () => motionQuery.matches);
  const selected = location.pathname.startsWith("/clipboard") ? "clipboard" : "";
  const clipboardItem = { key: "clipboard", icon: <HistoryOutlined />, label: t("zdx.label.nav.clipboard") };

  useEffect(() => {
    if (!isDesktop()) return;
    let stop: (() => void) | undefined;
    let disposed = false;
    const open = (request: { path: string; enable: boolean }) => {
      const path = request.path === "/clipboard/settings" ? "/clipboard/settings" : "/clipboard";
      navigate(path, { state: request.enable ? { enable: true } : null });
    };
    void listen<{ path: string; enable: boolean }>("clipboard://open-main", (event) => {
      open(event.payload);
      void takeClipboardMainRequest();
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    void takeClipboardMainRequest().then((request) => {
      if (request) open(request);
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [navigate]);

  return (
    <Layout className="h-full overflow-hidden" style={{ height: "100%" }}>
      <Layout.Sider
        theme="light"
        collapsible
        collapsed={collapsed}
        onCollapse={setCollapsed}
        width={232}
        collapsedWidth={72}
        trigger={null}
        style={{
          height: "100%",
          background: token.colorBgLayout,
          borderInlineEnd: `1px solid ${token.colorBorderSecondary}`,
          transition: reduceMotion ? "none" : undefined,
        }}
      >
        <Flex vertical style={{ height: "100%" }}>
          <Flex align="center" justify={collapsed ? "center" : "flex-start"} gap={collapsed ? 0 : 12} style={{ padding: collapsed ? "24px 0 8px" : "24px 20px 8px" }}>
            <img src="/pixo-icon.png" alt="" className="h-10 w-10 rounded-xl" />
            {!collapsed && (
              <Flex vertical gap={0}>
                <Typography.Title level={4} style={{ margin: 0, color: token.colorText }}>Pixo</Typography.Title>
                <Typography.Text style={{ color: token.colorTextSecondary }}>{t("zdx.text.app.tagline")}</Typography.Text>
              </Flex>
            )}
          </Flex>
          <Menu
            mode="inline"
            inlineCollapsed={collapsed}
            selectedKeys={selected ? [selected] : []}
            style={{ borderInlineEnd: 0, background: "transparent" }}
            onClick={({ key }) => {
              if (key === "clipboard") navigate("/clipboard");
            }}
            items={collapsed ? [clipboardItem] : [{ type: "group", label: t("zdx.label.nav.workspace"), children: [clipboardItem] }]}
          />
          {!collapsed && <Typography.Text style={{ marginTop: "auto", padding: "12px 20px", color: token.colorTextSecondary }}>{t("zdx.text.app.local")}</Typography.Text>}
        </Flex>
      </Layout.Sider>
      <Layout className="min-h-0 overflow-hidden">
        <Layout.Header style={{ display: "flex", alignItems: "center", justifyContent: "space-between", paddingInline: 16, lineHeight: 1, background: token.colorBgLayout, color: token.colorText, borderBottom: `1px solid ${token.colorBorderSecondary}` }}>
          <Flex align="center" gap={8} style={{ flexShrink: 0 }}>
            <Button
              type="text"
              aria-label={collapsed ? t("zdx.label.nav.expand") : t("zdx.label.nav.collapse")}
              icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
              onClick={() => setCollapsed((value) => !value)}
              style={{ color: token.colorText }}
            />
            <Typography.Text type="secondary" style={{ whiteSpace: "nowrap" }}>{t("zdx.text.app.welcome")}</Typography.Text>
          </Flex>
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
        <PermissionPrompt />
      </Layout>
    </Layout>
  );
}
