import { getVersion } from "@tauri-apps/api/app";

import { checkForUpdate, type UpdateStatus } from "@/api/system";
import { isDesktop } from "@/utils/desktop";

export const useUpdateStore = createZustandStore<{
  currentVersion: string;
  update: UpdateStatus | null;
  checking: boolean;
  error: boolean;
  promptOpen: boolean;
  check: () => Promise<void>;
  closePrompt: () => void;
}>()((set, get) => ({
  currentVersion: "",
  update: null,
  checking: false,
  error: false,
  promptOpen: false,
  check: async () => {
    if (!isDesktop() || get().checking) return;
    set({ checking: true, error: false });
    if (!get().currentVersion) {
      const currentVersion = await getVersion().catch(() => "");
      set({ currentVersion });
    }
    try {
      const update = await checkForUpdate();
      set({ update, checking: false, promptOpen: update.updateAvailable });
    } catch {
      set({ update: null, checking: false, error: true, promptOpen: false });
    }
  },
  closePrompt: () => set({ promptOpen: false }),
}));
