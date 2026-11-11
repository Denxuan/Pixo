import { getCurrentWindow } from "@tauri-apps/api/window";
import { setBackground, setTrayIcon } from "@/api/system";

import BootScreen from "@/components/BootScreen";
import { useSharedSettings } from "@/hooks/useSharedSettings";
import { useSettingsStore } from "@/stores/settings";
import { isTrayIcon } from "@/components/icons/TrayMark";
import { isDesktop } from "@/utils/desktop";
import { isDarkTheme } from "@/utils/theme";

const ClipboardPanel = lazy(() => import("@/pages/ClipboardPanel"));
const AppRouter = lazy(() => import("@/router"));

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const subscribe = (onChange: () => void) => {
  colorScheme.addEventListener("change", onChange);
  return () => colorScheme.removeEventListener("change", onChange);
};

function isClipboardPanel() {
  if (!isDesktop()) return false;
  try {
    return getCurrentWindow().label === "clipboard-panel";
  } catch {
    return false;
  }
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
  const dark = isDarkTheme(mode, systemDark);
  const panel = isClipboardPanel();
  const hydrated = useSettingsHydrated();
  useSharedSettings(!panel);

  useEffect(() => {
    if (!isDesktop() || panel || !hydrated) return;
    void setBackground(useSettingsStore.getState().background).catch((error) => {
      console.error("Failed to apply background mode", error);
      useSettingsStore.getState().setBackground(false);
    });
    const trayIcon = useSettingsStore.getState().trayIcon;
    void setTrayIcon(isTrayIcon(trayIcon) ? trayIcon : "board").catch((error) => {
      console.error("Failed to apply taskbar icon", error);
    });
  }, [hydrated, panel]);

  useEffect(() => {
    if (!isDesktop() || panel || !hydrated) return;
    const current = getCurrentWindow();
    void current.setTheme(mode === "system" ? null : dark ? "dark" : "light").catch(() => undefined);
    void current.setBackgroundColor(dark ? "#000000" : "#f5f5f5").catch(() => undefined);
  }, [dark, hydrated, mode, panel]);

  return (
    <ConfigProvider theme={{ algorithm: dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm }}>
      <div className={dark ? "dark h-full overflow-hidden" : "h-full overflow-hidden"} style={{ colorScheme: dark ? "dark" : "light", background: panel ? "transparent" : undefined }}>
        <Suspense fallback={panel ? null : <BootScreen />}>
          {panel ? <ClipboardPanel /> : <AppRouter />}
        </Suspense>
      </div>
    </ConfigProvider>
  );
}
