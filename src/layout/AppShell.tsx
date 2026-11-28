import { listen } from "@tauri-apps/api/event";

import { clipboardStatus, takeClipboardMainRequest } from "@/api/clipboard";
import { dockerStatus } from "@/api/docker";
import { screenshotGetSettings } from "@/api/screenshot";
import { sshStatus } from "@/api/ssh";
import FirstRun from "@/components/FirstRun";
import Launcher from "@/components/Launcher";
import PermissionPrompt from "@/components/PermissionPrompt";
import UpdatePrompt from "@/components/UpdatePrompt";
import type { ClipboardStatus } from "@/types/clipboard";
import { isDesktop } from "@/utils/desktop";
import { inkFrameStyle } from "@/utils/theme";
import { redirectForClosedTool } from "@/utils/toolEntry";

const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
const subscribeMotion = (onChange: () => void) => {
  motionQuery.addEventListener("change", onChange);
  return () => motionQuery.removeEventListener("change", onChange);
};

function mainWindowPath(path: string) {
  const clean = path.split("?")[0];
  if (clean === "/settings" || clean === "/settings/clipboard" || clean === "/settings/screenshot") return clean;
  if (clean === "/clipboard/settings") return "/settings/clipboard";
  if (clean === "/settings/ssh" || clean === "/settings/docker" || clean === "/settings/data") return clean;
  if (clean === "/clipboard" || clean === "/screenshot" || clean === "/ssh" || clean === "/docker") return clean;
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
  const [launcherOpen, setLauncherOpen] = useState(false);
  const collapsedByTool = useRef(false);
  const [clipboardOn, setClipboardOn] = useState<boolean | null>(null);
  const [screenshotOn, setScreenshotOn] = useState<boolean | null>(null);
  const [sshOn, setSshOn] = useState<boolean | null>(null);
  const [dockerOn, setDockerOn] = useState<boolean | null>(null);
  const reduceMotion = useSyncExternalStore(subscribeMotion, () => motionQuery.matches);
  const onSettings = location.pathname.startsWith("/settings");
  const selected = location.pathname.startsWith("/screenshot")
    ? "screenshot"
    : location.pathname.startsWith("/ssh")
      ? "ssh"
      : location.pathname.startsWith("/docker")
        ? "docker"
        : location.pathname.startsWith("/clipboard")
        ? "clipboard"
        : "";
  const clipboardItem = { key: "clipboard", icon: <HistoryOutlined />, label: t("zdx.label.nav.clipboard") };
  const screenshotItem = { key: "screenshot", icon: <ScreenshotNavIcon />, label: t("zdx.label.nav.screenshot") };
  const sshItem = { key: "ssh", icon: <CloudServerOutlined />, label: t("zdx.label.nav.ssh") };
  const dockerItem = { key: "docker", icon: <DockerOutlined />, label: t("zdx.label.nav.docker") };
  const showClipboard = !isDesktop() || clipboardOn !== false;
  const showScreenshot = !isDesktop() || screenshotOn !== false;
  const showSsh = !isDesktop() || sshOn !== false;
  const showDocker = !isDesktop() || dockerOn !== false;
  const navItems = [
    ...(showClipboard ? [clipboardItem] : []),
    ...(showScreenshot ? [screenshotItem] : []),
    ...(showSsh ? [sshItem] : []),
    ...(showDocker ? [dockerItem] : []),
  ];

  useEffect(() => {
    if (!isDesktop()) return;
    let stop: (() => void) | undefined;
    let stopLauncher: (() => void) | undefined;
    let disposed = false;
    const open = (request: { path: string; enable: boolean; hostId?: number | null }) => {
      const hostId = typeof request.hostId === "number" ? request.hostId : null;
      navigate(mainWindowPath(request.path), { state: request.enable || hostId ? { enable: request.enable, hostId } : null });
    };
    void listen("launcher://toggle", () => {
      setLauncherOpen((open) => !open);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopLauncher = unlisten;
    });
    void listen<{ path: string; enable: boolean; hostId?: number | null }>("clipboard://open-main", (event) => {
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
      stopLauncher?.();
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
    void sshStatus().then((status) => {
      if (!disposed) setSshOn(status.enabled);
    }).catch(() => undefined);
    void dockerStatus().then((status) => {
      if (!disposed) setDockerOn(status.enabled);
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
    let stopSsh: (() => void) | undefined;
    void listen<boolean>("ssh://enabled", (event) => {
      setSshOn(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopSsh = unlisten;
    });
    let stopDocker: (() => void) | undefined;
    void listen<boolean>("docker://enabled", (event) => {
      setDockerOn(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopDocker = unlisten;
    });
    return () => {
      disposed = true;
      stopStatus?.();
      stopScreenshot?.();
      stopSsh?.();
      stopDocker?.();
    };
  }, []);

  useEffect(() => {
    if (!isDesktop()) return;
    const next = redirectForClosedTool(location.pathname, {
      clipboard: clipboardOn,
      screenshot: screenshotOn,
      ssh: sshOn,
      docker: dockerOn,
    });
    if (next) navigate(next, { replace: true });
  }, [clipboardOn, dockerOn, location.pathname, navigate, screenshotOn, sshOn]);

  useEffect(() => {
    const onTool = location.pathname.startsWith("/ssh") || location.pathname.startsWith("/docker");
    if (onTool) {
      setCollapsed((value) => {
        if (!value) collapsedByTool.current = true;
        return true;
      });
      return;
    }
    if (collapsedByTool.current) {
      collapsedByTool.current = false;
      setCollapsed(false);
    }
  }, [location.pathname]);

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
        style={{ height: "100%", borderInlineEnd: `${token.lineWidth}px solid ${token.colorBorder}`, transition: reduceMotion ? "none" : undefined }}
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
            className="pixo-nav min-h-0 flex-1 overflow-y-auto overflow-x-hidden"
            style={{ borderInlineEnd: 0, background: "transparent" }}
            onClick={({ key }) => {
              if (key === "clipboard") navigate("/clipboard");
              if (key === "screenshot") navigate("/screenshot");
              if (key === "ssh") navigate("/ssh");
              if (key === "docker") navigate("/docker");
            }}
            items={collapsed ? navItems : [{ type: "group", label: t("zdx.label.nav.tools"), children: navItems }]}
          />
          <div className="shrink-0 pb-3">
            <Menu
              mode="inline"
              inlineCollapsed={collapsed}
              selectedKeys={onSettings ? ["settings"] : []}
              className="pixo-nav"
              style={{ borderInlineEnd: 0, background: "transparent" }}
              onClick={({ key }) => {
                if (key === "search") setLauncherOpen(true);
                if (key === "settings") navigate("/settings");
              }}
              items={[
                { key: "search", icon: <SearchOutlined />, label: t("zdx.label.launcher.open") },
                { key: "settings", icon: <SettingOutlined />, label: t("zdx.label.nav.settings") },
              ]}
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
      <Layout.Content className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden p-3 pl-2">
        <main
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
          style={{ background: token.colorBgContainer, ...inkFrameStyle(token, token.borderRadiusLG) }}
        >
          <Outlet />
        </main>
      </Layout.Content>
      <Launcher
        open={launcherOpen}
        clipboardOn={clipboardOn}
        screenshotOn={screenshotOn}
        sshOn={sshOn}
        dockerOn={dockerOn}
        onClose={() => setLauncherOpen(false)}
      />
      <PermissionPrompt />
      <UpdatePrompt />
      <FirstRun />
    </Layout>
  );
}
