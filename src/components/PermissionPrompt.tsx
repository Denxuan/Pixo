import { listen } from "@tauri-apps/api/event";

import { permissionsOpen, permissionsPrepare, permissionsStatus, type SystemPermission } from "@/api/permissions";
import { clipboardStatus, toClipboardError } from "@/api/clipboard";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";
import { permissionPromptHeld } from "@/utils/permissionGate";
import { useSettingsStore } from "@/stores/settings";
import type { ClipboardStatus } from "@/types/clipboard";

const permissionCopy = {
  accessibility: {
    label: "zdx.label.permission.accessibility",
    body: "zdx.text.permission.accessibility",
  },
  inputMonitoring: {
    label: "zdx.label.permission.inputMonitoring",
    body: "zdx.text.permission.inputMonitoring",
  },
  pasteboard: {
    label: "zdx.label.permission.pasteboard",
    body: "zdx.text.permission.pasteboard",
  },
} as const;

function isPermissionId(id: string): id is SystemPermission["id"] {
  return id in permissionCopy;
}

export default function PermissionPrompt() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const introduced = useSettingsStore((state) => state.introduced);
  const [clipboardOn, setClipboardOn] = useState(false);
  const [items, setItems] = useState<SystemPermission[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [error, setError] = useState("");
  const missing = items.filter((item) => !item.granted && isPermissionId(item.id));
  const open = desktop && introduced && clipboardOn && !permissionPromptHeld() && !dismissed && missing.length > 0;

  useEffect(() => {
    if (!desktop || !introduced) return;
    let alive = true;
    let stop: (() => void) | undefined;
    void clipboardStatus().then((status) => {
      if (alive) setClipboardOn(status.enabled);
    }).catch(() => undefined);
    void listen<ClipboardStatus>("clipboard://status", (event) => {
      setClipboardOn(event.payload.enabled);
    }).then((unlisten) => {
      if (alive) stop = unlisten;
      else unlisten();
    });
    return () => {
      alive = false;
      stop?.();
    };
  }, [desktop, introduced]);

  useEffect(() => {
    if (!desktop || !introduced || !clipboardOn) return;
    let alive = true;
    const apply = (next: SystemPermission[]) => {
      if (alive) setItems(next);
    };
    void permissionsStatus()
      .then((current) => {
        apply(current);
        if (current.every((item) => item.granted)) return;
        requestAnimationFrame(() => {
          void permissionsPrepare().then(apply).catch(() => undefined);
        });
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [clipboardOn, desktop, introduced]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    const timer = window.setInterval(() => {
      void permissionsStatus()
        .then((current) => {
          if (alive) setItems(current);
        })
        .catch(() => undefined);
    }, 1500);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [open]);

  return (
    <Modal
      open={open}
      title={t("zdx.label.permission.title")}
      onCancel={() => setDismissed(true)}
      maskClosable={false}
      footer={<Button autoInsertSpace={false} onClick={() => setDismissed(true)}>{t("zdx.label.permission.later")}</Button>}
    >
      <Flex vertical gap={16}>
        <Typography.Paragraph type="secondary" style={{ margin: 0 }}>{t("zdx.text.permission.intro")}</Typography.Paragraph>
        <SettingGroup>
          {missing.map((item) => (
            <SettingRow key={item.id} label={t(permissionCopy[item.id].label)} description={t(permissionCopy[item.id].body)}>
              <Button
                size="small"
                type="primary"
                ghost
                onClick={() => {
                  setError("");
                  void permissionsOpen(item.id).catch((caught) => {
                    setError(clipboardErrorText(t, toClipboardError(caught)));
                  });
                }}
              >
                {t("zdx.label.permission.openSettings")}
              </Button>
            </SettingRow>
          ))}
        </SettingGroup>
        <Typography.Text type="secondary" className="text-xs">{t("zdx.text.permission.restart")}</Typography.Text>
        {error && <Alert type="error" showIcon message={error} />}
      </Flex>
    </Modal>
  );
}
