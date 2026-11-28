import { getCurrentWindow } from "@tauri-apps/api/window";
import { setBackground, setLauncherShortcut, setTrayIcon } from "@/api/system";

import BootScreen from "@/components/BootScreen";
import { useSharedSettings } from "@/hooks/useSharedSettings";
import { useSettingsStore } from "@/stores/settings";
import { isTrayIcon } from "@/components/icons/TrayMark";
import { isDesktop } from "@/utils/desktop";
import { ACCENT, ILLUSTRATION, illustrationConfig, illustrationPalette, isDarkTheme, SHELL_BACKGROUND } from "@/utils/theme";

const ClipboardPanel = lazy(() => import("@/pages/ClipboardPanel"));
const ScreenshotOverlay = lazy(() => import("@/pages/ScreenshotOverlay"));
const ScreenshotPin = lazy(() => import("@/pages/ScreenshotPin"));
const ScreenshotCamera = lazy(() => import("@/pages/ScreenshotCamera"));
const ScreenshotRecorder = lazy(() => import("@/pages/ScreenshotRecorder"));
const ScreenshotScrollHud = lazy(() => import("@/pages/ScreenshotScrollHud"));
const AppRouter = lazy(() => import("@/router"));

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const subscribe = (onChange: () => void) => {
  colorScheme.addEventListener("change", onChange);
  return () => colorScheme.removeEventListener("change", onChange);
};

function useSurface(): "app" | "panel" | "overlay" | "pin" | "recorder" | "camera" | "scroll" {
  if (!isDesktop()) return new URLSearchParams(window.location.search).get("view") === "panel" ? "panel" : "app";
  try {
    const label = getCurrentWindow().label;
    if (label === "clipboard-panel") return "panel";
    if (label === "screenshot-recorder") return "recorder";
    if (label === "screenshot-camera") return "camera";
    if (label === "screenshot-scroll") return "scroll";
    if (label.startsWith("screenshot-overlay-")) return "overlay";
    if (label.startsWith("screenshot-pin-")) return "pin";
  } catch {
    return "app";
  }
  return "app";
}

function themeProps(illustrated: boolean, dark: boolean) {
  const algorithm = dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm;
  if (!illustrated) return { button: { autoInsertSpace: false }, theme: { algorithm } };
  const skin = illustrationConfig(dark);
  return { ...skin, theme: { ...skin.theme, algorithm } };
}

function IllustrationVars({ active, canvas, dark }: { active: boolean; canvas: boolean; dark: boolean }) {
  useLayoutEffect(() => {
    const root = document.documentElement;
    const names = ["--pixo-ink", "--pixo-muted", "--pixo-line", "--pixo-radius", "--pixo-radius-lg", "--pixo-paper", "--pixo-primary", "--pixo-sider", "--pixo-border", "--pixo-hover", "--pixo-shadow", "--pixo-shadow-sm"];
    if (!active) {
      for (const name of names) root.style.removeProperty(name);
      root.style.removeProperty("background");
      root.style.removeProperty("color-scheme");
      return;
    }
    const palette = illustrationPalette(dark);
    root.style.setProperty("--pixo-ink", palette.ink);
    root.style.setProperty("--pixo-muted", palette.muted);
    root.style.setProperty("--pixo-line", `${ILLUSTRATION.line}px`);
    root.style.setProperty("--pixo-radius", `${ILLUSTRATION.radius}px`);
    root.style.setProperty("--pixo-radius-lg", `${ILLUSTRATION.radiusLG}px`);
    root.style.setProperty("--pixo-paper", palette.container);
    root.style.setProperty("--pixo-primary", ACCENT);
    root.style.setProperty("--pixo-sider", palette.sider);
    root.style.setProperty("--pixo-border", palette.stroke);
    root.style.setProperty("--pixo-hover", palette.hoverBg);
    root.style.setProperty("--pixo-shadow", `${ILLUSTRATION.shadow}px`);
    root.style.setProperty("--pixo-shadow-sm", `${ILLUSTRATION.shadowSm}px`);
    if (canvas) root.style.background = palette.paper;
    else root.style.removeProperty("background");
    root.style.colorScheme = dark ? "dark" : "light";
  }, [active, canvas, dark]);
  return null;
}

function ScrollbarTone() {
  const { token } = antdTheme.useToken();
  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--pixo-scrollbar", token.colorTextTertiary);
    root.style.setProperty("--pixo-scrollbar-hover", token.colorTextSecondary);
  }, [token.colorTextSecondary, token.colorTextTertiary]);
  return null;
}

function useSettingsHydrated() {
  return useSyncExternalStore(
    (onChange) => useSettingsStore.persist.onFinishHydration(onChange),
    () => useSettingsStore.persist.hasHydrated(),
    () => true,
  );
}

export default function App() {
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(subscribe, () => colorScheme.matches);
  const surface = useSurface();
  const appWindow = surface === "app";
  const hydrated = useSettingsHydrated();
  useSharedSettings(appWindow);
  const dark = surface === "overlay" || surface === "pin" || isDarkTheme(mode, systemDark);

  useEffect(() => {
    if (!isDesktop() || !appWindow || !hydrated) return;
    void setBackground(useSettingsStore.getState().background).catch((error) => {
      console.error("Failed to apply background mode", error);
      useSettingsStore.getState().setBackground(false);
    });
    const trayIcon = useSettingsStore.getState().trayIcon;
    void setTrayIcon(isTrayIcon(trayIcon) ? trayIcon : "board").catch((error) => {
      console.error("Failed to apply taskbar icon", error);
    });
    void setLauncherShortcut(useSettingsStore.getState().launcherShortcut).catch((error) => {
      console.error("Failed to register launcher shortcut", error);
    });
  }, [appWindow, hydrated]);

  useEffect(() => {
    if (!isDesktop() || !appWindow || !hydrated) return;
    const current = getCurrentWindow();
    void current.setTheme(mode === "system" ? null : dark ? "dark" : "light").catch(() => undefined);
    void current.setBackgroundColor(dark ? SHELL_BACKGROUND.dark : SHELL_BACKGROUND.light).catch(() => undefined);
  }, [appWindow, dark, hydrated, mode]);

  const illustrated = appWindow || surface === "panel";
  const provider = themeProps(illustrated, dark);

  useEffect(() => {
    const props = themeProps(illustrated, dark);
    ConfigProvider.config({
      holderRender: (children) => <ConfigProvider {...props}>{children}</ConfigProvider>,
      theme: props.theme,
    });
  }, [dark, illustrated]);

  return (
    <ConfigProvider {...provider}>
      <IllustrationVars active={illustrated} canvas={appWindow} dark={dark} />
      <ScrollbarTone />
      <div className={dark ? "dark h-full overflow-hidden" : "h-full overflow-hidden"} style={{ colorScheme: dark ? "dark" : "light", background: surface === "panel" || surface === "recorder" || surface === "camera" || surface === "overlay" ? "transparent" : surface === "scroll" ? "#1c1c1c" : appWindow ? undefined : "#000" }}>
        <Suspense fallback={appWindow ? <BootScreen /> : null}>
          {surface === "recorder" ? <ScreenshotRecorder /> : surface === "camera" ? <ScreenshotCamera /> : surface === "scroll" ? <ScreenshotScrollHud /> : surface === "overlay" ? <ScreenshotOverlay /> : surface === "pin" ? <ScreenshotPin /> : surface === "panel" ? <ClipboardPanel /> : <AppRouter />}
        </Suspense>
      </div>
    </ConfigProvider>
  );
}
