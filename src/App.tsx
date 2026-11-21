import { getCurrentWindow } from "@tauri-apps/api/window";
import { setBackground, setTrayIcon } from "@/api/system";

import BootScreen from "@/components/BootScreen";
import { useSharedSettings } from "@/hooks/useSharedSettings";
import { useSettingsStore } from "@/stores/settings";
import { isTrayIcon } from "@/components/icons/TrayMark";
import { isDesktop } from "@/utils/desktop";
import { appTheme, isDarkTheme, SHELL_BACKGROUND } from "@/utils/theme";

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
  if (!isDesktop()) return "app";
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
  }, [appWindow, hydrated]);

  useEffect(() => {
    if (!isDesktop() || !appWindow || !hydrated) return;
    const current = getCurrentWindow();
    void current.setTheme(mode === "system" ? null : dark ? "dark" : "light").catch(() => undefined);
    void current.setBackgroundColor(dark ? SHELL_BACKGROUND.dark : SHELL_BACKGROUND.light).catch(() => undefined);
  }, [appWindow, dark, hydrated, mode]);

  return (
    <ConfigProvider button={{ autoInsertSpace: false }} theme={{ ...(appWindow ? appTheme(dark) : {}), algorithm: dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm }}>
      <div className={dark ? "dark h-full overflow-hidden" : "h-full overflow-hidden"} style={{ colorScheme: dark ? "dark" : "light", background: surface === "panel" || surface === "recorder" || surface === "camera" || surface === "overlay" ? "transparent" : surface === "scroll" ? "#1c1c1c" : appWindow ? undefined : "#000" }}>
        <Suspense fallback={appWindow ? <BootScreen /> : null}>
          {surface === "recorder" ? <ScreenshotRecorder /> : surface === "camera" ? <ScreenshotCamera /> : surface === "scroll" ? <ScreenshotScrollHud /> : surface === "overlay" ? <ScreenshotOverlay /> : surface === "pin" ? <ScreenshotPin /> : surface === "panel" ? <ClipboardPanel /> : <AppRouter />}
        </Suspense>
      </div>
    </ConfigProvider>
  );
}
