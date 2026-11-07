import { getCurrentWindow } from "@tauri-apps/api/window";

import ClipboardPanel from "@/pages/ClipboardPanel";
import AppRouter from "@/router";
import { useSettingsStore } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";
import { isDarkTheme } from "@/utils/theme";

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

export default function App() {
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(subscribe, () => colorScheme.matches);
  const dark = isDarkTheme(mode, systemDark);
  const panel = isClipboardPanel();

  return (
    <ConfigProvider theme={{ algorithm: dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm }}>
      <div className={dark ? "dark h-screen overflow-hidden" : "h-screen overflow-hidden"} style={{ colorScheme: dark ? "dark" : "light" }}>
        {panel ? <ClipboardPanel /> : <AppRouter />}
      </div>
    </ConfigProvider>
  );
}
