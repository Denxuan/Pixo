import Workspace from "@/pages/Workspace";
import { useSettingsStore } from "@/stores/settings";
import { isDarkTheme } from "@/utils/theme";

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const subscribe = (onChange: () => void) => {
  colorScheme.addEventListener("change", onChange);
  return () => colorScheme.removeEventListener("change", onChange);
};

export default function App() {
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(subscribe, () => colorScheme.matches);
  const dark = isDarkTheme(mode, systemDark);

  return (
    <ConfigProvider theme={{ algorithm: dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm }}>
      <div className={dark ? "dark" : ""} style={{ colorScheme: dark ? "dark" : "light" }}>
        <Workspace />
      </div>
    </ConfigProvider>
  );
}
