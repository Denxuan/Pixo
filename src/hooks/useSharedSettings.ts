import { emit, listen } from "@tauri-apps/api/event";

import { useSettingsStore, type Language, type ThemeMode } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";

type SharedSettings = {
  mode: ThemeMode;
  language: Language;
};

const SETTINGS_EVENT = "pixo://settings";
const SETTINGS_REQUEST = "pixo://settings-request";

function currentSettings(): SharedSettings {
  const state = useSettingsStore.getState();
  return { mode: state.mode, language: state.language };
}

function isSharedSettings(value: unknown): value is SharedSettings {
  if (!value || typeof value !== "object") return false;
  const record = value as Partial<SharedSettings>;
  const mode = record.mode === "system" || record.mode === "light" || record.mode === "dark";
  const language = record.language === "zh-CN" || record.language === "en";
  return mode && language;
}

function applySettings(next: SharedSettings) {
  const state = useSettingsStore.getState();
  if (state.mode === next.mode && state.language === next.language) return;
  useSettingsStore.setState({ mode: next.mode, language: next.language });
}

export function useSharedSettings(owner: boolean) {
  const mode = useSettingsStore((state) => state.mode);
  const language = useSettingsStore((state) => state.language);
  const [hydrated, setHydrated] = useState(useSettingsStore.persist.hasHydrated());

  useEffect(() => {
    if (hydrated) return;
    return useSettingsStore.persist.onFinishHydration(() => setHydrated(true));
  }, [hydrated]);

  useEffect(() => {
    if (!isDesktop() || !hydrated) return;
    let alive = true;
    const stops: Array<() => void> = [];
    void listen<unknown>(SETTINGS_EVENT, (event) => {
      if (isSharedSettings(event.payload)) applySettings(event.payload);
    }).then((stop) => {
      if (!alive) {
        stop();
        return;
      }
      stops.push(stop);
      if (!owner) void emit(SETTINGS_REQUEST);
    });
    if (owner) {
      void listen(SETTINGS_REQUEST, () => {
        void emit(SETTINGS_EVENT, currentSettings());
      }).then((stop) => {
        if (alive) stops.push(stop);
        else stop();
      });
    }
    return () => {
      alive = false;
      stops.forEach((stop) => stop());
    };
  }, [hydrated, owner]);

  useEffect(() => {
    if (!isDesktop() || !hydrated || !owner) return;
    void emit(SETTINGS_EVENT, { mode, language });
  }, [hydrated, owner, mode, language]);

  useEffect(() => {
    if (!isDesktop() || !hydrated || owner) return;
    const request = () => {
      void emit(SETTINGS_REQUEST);
    };
    window.addEventListener("focus", request);
    return () => window.removeEventListener("focus", request);
  }, [hydrated, owner]);
}
