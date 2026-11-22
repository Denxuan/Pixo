import { SshRequestError, sshConfigImport, sshConfigPreview } from "@/api/ssh";
import type { SshConfigPreview } from "@/types/ssh";
import { sshErrorText } from "@/utils/sshMessage";

export default function ImportConfig({
  open,
  onClose,
  onImported,
}: {
  open: boolean;
  onClose: () => void;
  onImported: () => void;
}) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<SshConfigPreview[]>([]);
  const [selected, setSelected] = useState<number[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setBusy(true);
    setError("");
    sshConfigPreview()
      .then((preview) => {
        if (cancelled) return;
        setRows(preview);
        setSelected(preview.filter((row) => row.selected).map((row) => row.index));
      })
      .catch((caught: unknown) => {
        if (cancelled) return;
        setRows([]);
        setError(caught instanceof SshRequestError ? sshErrorText(t, caught) : t("zdx.text.ssh.error.unknown"));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, t]);

  async function submit() {
    setBusy(true);
    setError("");
    try {
      await sshConfigImport(selected);
      onImported();
      onClose();
    } catch (caught) {
      setError(caught instanceof SshRequestError ? sshErrorText(t, caught) : t("zdx.text.ssh.error.unknown"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={t("zdx.text.ssh.importTitle")}
      open={open}
      centered
      confirmLoading={busy}
      okText={t("zdx.label.ssh.import")}
      cancelText={t("zdx.label.ssh.cancel")}
      okButtonProps={{ disabled: selected.length === 0 }}
      destroyOnHidden
      styles={{ body: { maxHeight: "calc(100vh - 220px)", overflowY: "auto" } }}
      onCancel={onClose}
      onOk={() => void submit()}
    >
      <Flex vertical gap={12}>
        <Typography.Paragraph type="secondary" className="!mb-0">{t("zdx.text.ssh.importHint")}</Typography.Paragraph>
        {error && <Alert type="error" showIcon message={error} />}
        {busy && rows.length === 0 && !error && <Spin />}
        {!busy && rows.length === 0 && !error && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.ssh.importEmpty")} />}
        <Checkbox.Group className="w-full" value={selected} onChange={(value) => setSelected(value.map(Number))}>
          <Flex vertical gap={8} className="w-full">
            {rows.map((row) => (
              <Checkbox key={row.index} value={row.index} className="w-full">
                <div className="min-w-0">
                  <div className="font-medium">{row.name}</div>
                  <div className="text-xs" style={{ opacity: 0.7 }}>{row.username}@{row.host}:{row.port}</div>
                  {row.jumpNames.length > 0 && <div className="text-xs">{t("zdx.text.ssh.importJumps", { names: row.jumpNames.join(", ") })}</div>}
                  {row.warnings.map((warning) => (
                    <div key={warning} className="text-xs" style={{ opacity: 0.75 }}>{warningText(t, warning)}</div>
                  ))}
                </div>
              </Checkbox>
            ))}
          </Flex>
        </Checkbox.Group>
      </Flex>
    </Modal>
  );
}

function warningText(t: (key: string, options?: Record<string, unknown>) => string, warning: string) {
  if (warning === "duplicate") return t("zdx.text.ssh.importDuplicate");
  if (warning === "jump_missing") return t("zdx.text.ssh.importJumpMissing");
  if (warning === "identity_missing") return t("zdx.text.ssh.importIdentityMissing");
  if (warning.startsWith("unsupported:")) return t("zdx.text.ssh.importUnsupported", { detail: warning.slice("unsupported:".length) });
  return warning;
}
