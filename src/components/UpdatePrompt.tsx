import { openUrl } from "@tauri-apps/plugin-opener";

import { useUpdateStore } from "@/stores/update";
import { isDesktop } from "@/utils/desktop";

export default function UpdatePrompt() {
  const { t } = useTranslation();
  const currentVersion = useUpdateStore((state) => state.currentVersion);
  const update = useUpdateStore((state) => state.update);
  const promptOpen = useUpdateStore((state) => state.promptOpen);
  const check = useUpdateStore((state) => state.check);
  const closePrompt = useUpdateStore((state) => state.closePrompt);

  useEffect(() => {
    if (!isDesktop()) return;
    void check();
  }, [check]);

  return (
    <Modal
      open={promptOpen && Boolean(update?.updateAvailable)}
      title={t("zdx.text.system.updateTitle")}
      okText={t("zdx.label.system.install")}
      cancelText={t("zdx.label.system.later")}
      onOk={async () => {
        if (!update?.url) return;
        await openUrl(update.url);
        closePrompt();
      }}
      onCancel={closePrompt}
    >
      <Typography.Paragraph style={{ marginBottom: 0 }}>
        {t("zdx.text.system.updatePrompt", { current: currentVersion, latest: update?.latest ?? "" })}
      </Typography.Paragraph>
    </Modal>
  );
}
