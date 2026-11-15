import { listen } from "@tauri-apps/api/event";

import { clipboardStatus, takeClipboardMainRequest } from "@/api/clipboard";
import { screenshotGetSettings } from "@/api/screenshot";
import PermissionPrompt from "@/components/PermissionPrompt";
import UpdatePrompt from "@/components/UpdatePrompt";
import { useSettingsStore, type ThemeMode } from "@/stores/settings";
import type { ClipboardStatus } from "@/types/clipboard";
import { isDesktop } from "@/utils/desktop";

const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
const subscribeMotion = (onChange: () => void) => {
  motionQuery.addEventListener("change", onChange);
  return () => motionQuery.removeEventListener("change", onChange);
};

function mainWindowPath(path: string) {
  if (path === "/settings" || path === "/settings/clipboard" || path === "/settings/screenshot") return path;
  if (path === "/clipboard/settings") return "/settings/clipboard";
  return "/clipboard";
}

function isThemeMode(value: string): value is ThemeMode {
  return value === "system" || value === "light" || value === "dark";
}

function ScreenshotNavIcon() {
  return (
    <span className="anticon" aria-hidden="true">
      <svg width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
        <path d="M7 3H5a2 2 0 0 0-2 2v2M17 3h2a2 2 0 0 1 2 2v2M7 21H5a2 2 0 0 1-2-2v-2M17 21h2a2 2 0 0 0 2-2v-2M4 12h16" />
      </svg>
    </span>
  );
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
  const [clipboardOn, setClipboardOn] = useState<boolean | null>(null);
  const [screenshotOn, setScreenshotOn] = useState<boolean | null>(null);
  const reduceMotion = useSyncExternalStore(subscribeMotion, () => motionQuery.matches);
  const selected = location.pathname.startsWith("/settings") ? "settings" : location.pathname.startsWith("/screenshot") ? "screenshot" : location.pathname.startsWith("/clipboard") ? "clipboard" : "";
  const clipboardItem = { key: "clipboard", icon: <HistoryOutlined />, label: t("zdx.label.nav.clipboard") };
  const screenshotItem = { key: "screenshot", icon: <ScreenshotNavIcon />, label: t("zdx.label.nav.screenshot") };
  const settingsItem = { key: "settings", icon: <SettingOutlined />, label: t("zdx.label.system.title") };
  const showClipboard = !isDesktop() || clipboardOn !== false;
  const showScreenshot = !isDesktop() || screenshotOn !== false;
  const navItems = [
    ...(showClipboard ? [clipboardItem] : []),
    ...(showScreenshot ? [screenshotItem] : []),
    settingsItem,
  ];

  useEffect(() => {
    if (!isDesktop()) return;
    let stop: (() => void) | undefined;
    let disposed = false;
    const open = (request: { path: string; enable: boolean }) => {
      navigate(mainWindowPath(request.path), { state: request.enable ? { enable: true } : null });
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

  useEffect(() => {
    if (!isDesktop()) return;
    let disposed = false;
    let stopStatus: (() => void) | undefined;
    let stopScreenshot: (() => void) | undefined;
    void clipboardStatus().then((status) => {
      if (!disposed) setClipboardOn(status.enabled);
    }).catch(() => undefined);
    void screenshotGetSettings().then((settings) => {
      if (!disposed) setScreenshotOn(settings.enabled);
    }).catch(() => undefined);
    void listen<ClipboardStatus>("clipboard://status", (event) => {
      setClipboardOn(event.payload.enabled);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopStatus = unlisten;
    });
    void listen<boolean>("screenshot://enabled", (event) => {
      setScreenshotOn(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopScreenshot = unlisten;
    });
    return () => {
      disposed = true;
      stopStatus?.();
      stopScreenshot?.();
    };
  }, []);

  useEffect(() => {
    if (!isDesktop() || screenshotOn !== false || location.pathname !== "/screenshot") return;
    navigate("/settings", { replace: true });
  }, [location.pathname, navigate, screenshotOn]);

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
              if (key === "screenshot") navigate("/screenshot");
              if (key === "settings") navigate("/settings");
            }}
            items={collapsed ? navItems : [{ type: "group", label: t("zdx.label.nav.workspace"), children: navItems }]}
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
        <UpdatePrompt />
      </Layout>
    </Layout>
  );
}
