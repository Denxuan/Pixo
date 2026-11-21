import { listen } from "@tauri-apps/api/event";

import { clipboardStatus, takeClipboardMainRequest } from "@/api/clipboard";
import { screenshotGetSettings } from "@/api/screenshot";
import PermissionPrompt from "@/components/PermissionPrompt";
import UpdatePrompt from "@/components/UpdatePrompt";
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
  const { token } = antdTheme.useToken();
  const [collapsed, setCollapsed] = useState(false);
  const [clipboardOn, setClipboardOn] = useState<boolean | null>(null);
  const [screenshotOn, setScreenshotOn] = useState<boolean | null>(null);
  const reduceMotion = useSyncExternalStore(subscribeMotion, () => motionQuery.matches);
  const onSettings = location.pathname.startsWith("/settings");
  const selected = location.pathname.startsWith("/screenshot") ? "screenshot" : location.pathname.startsWith("/clipboard") ? "clipboard" : "";
  const clipboardItem = { key: "clipboard", icon: <HistoryOutlined />, label: t("zdx.label.nav.clipboard") };
  const screenshotItem = { key: "screenshot", icon: <ScreenshotNavIcon />, label: t("zdx.label.nav.screenshot") };
  const showClipboard = !isDesktop() || clipboardOn !== false;
  const showScreenshot = !isDesktop() || screenshotOn !== false;
  const navItems = [
    ...(showClipboard ? [clipboardItem] : []),
    ...(showScreenshot ? [screenshotItem] : []),
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
        width={216}
        collapsedWidth={64}
        trigger={null}
        style={{ height: "100%", background: "transparent", transition: reduceMotion ? "none" : undefined }}
      >
        <nav aria-label={t("zdx.label.nav.tools")} className="flex h-full flex-col">
          <div className={collapsed ? "flex shrink-0 justify-center px-0 pb-4 pt-5" : "flex shrink-0 items-center gap-2.5 px-5 pb-4 pt-5"}>
            <img src="/pixo-icon.png" alt="" className="h-8 w-8 shrink-0 rounded-lg" />
            {!collapsed && (
              <div className="min-w-0">
                <div className="truncate text-[15px] font-semibold leading-tight" style={{ color: token.colorText }}>Pixo</div>
                <div className="truncate text-xs leading-snug" style={{ color: token.colorTextTertiary }}>{t("zdx.text.app.tagline")}</div>
              </div>
            )}
          </div>
          <Menu
            mode="inline"
            inlineCollapsed={collapsed}
            selectedKeys={selected ? [selected] : []}
            className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden"
            style={{ borderInlineEnd: 0 }}
            onClick={({ key }) => {
              if (key === "clipboard") navigate("/clipboard");
              if (key === "screenshot") navigate("/screenshot");
            }}
            items={collapsed ? navItems : [{ type: "group", label: t("zdx.label.nav.tools"), children: navItems }]}
          />
          <div className="shrink-0 pb-3">
            <Menu
              mode="inline"
              inlineCollapsed={collapsed}
              selectedKeys={onSettings ? ["settings"] : []}
              style={{ borderInlineEnd: 0 }}
              onClick={() => navigate("/settings")}
              items={[{ key: "settings", icon: <SettingOutlined />, label: t("zdx.label.nav.settings") }]}
            />
            <div className={collapsed ? "flex justify-center pt-1" : "flex px-3 pt-1"}>
              <Tooltip title={collapsed ? t("zdx.label.nav.expand") : t("zdx.label.nav.collapse")} placement="right">
                <Button
                  type="text"
                  aria-label={collapsed ? t("zdx.label.nav.expand") : t("zdx.label.nav.collapse")}
                  icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
                  onClick={() => setCollapsed((value) => !value)}
                  style={{ color: token.colorTextSecondary }}
                />
              </Tooltip>
            </div>
          </div>
        </nav>
      </Layout.Sider>
      <Layout.Content className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden py-2 pr-2">
        <main
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
          style={{
            background: token.colorBgContainer,
            border: `1px solid ${token.colorBorderSecondary}`,
            borderRadius: token.borderRadiusLG,
            boxShadow: "0 1px 2px rgba(0, 0, 0, 0.04)",
          }}
        >
          <Outlet />
        </main>
      </Layout.Content>
      <PermissionPrompt />
      <UpdatePrompt />
    </Layout>
  );
}
