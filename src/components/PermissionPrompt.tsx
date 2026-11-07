import { permissionsOpen, permissionsPrepare, permissionsStatus, type SystemPermission } from "@/api/permissions";
import { toClipboardError } from "@/api/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";

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
  const [items, setItems] = useState<SystemPermission[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [error, setError] = useState("");
  const missing = items.filter((item) => !item.granted && isPermissionId(item.id));
  const open = desktop && !dismissed && missing.length > 0;

  useEffect(() => {
    if (!desktop) return;
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
  }, [desktop]);

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
        <Typography.Paragraph style={{ margin: 0 }}>{t("zdx.text.permission.intro")}</Typography.Paragraph>
        {missing.map((item) => (
          <Flex key={item.id} vertical gap={4}>
            <Flex justify="space-between" align="center" gap={12}>
              <Typography.Text strong>{t(permissionCopy[item.id].label)}</Typography.Text>
              <Button
                size="small"
                onClick={() => {
                  setError("");
                  void permissionsOpen(item.id).catch((caught) => {
                    setError(clipboardErrorText(t, toClipboardError(caught)));
                  });
                }}
              >
                {t("zdx.label.permission.openSettings")}
              </Button>
            </Flex>
            <Typography.Text type="secondary">{t(permissionCopy[item.id].body)}</Typography.Text>
          </Flex>
        ))}
        <Typography.Text type="secondary">{t("zdx.text.permission.restart")}</Typography.Text>
        {error && <Alert type="error" showIcon message={error} />}
      </Flex>
    </Modal>
  );
}
