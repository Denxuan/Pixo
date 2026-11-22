import { DockerRequestError, dockerContextImport, dockerContextPreview } from "@/api/docker";
import type { DockerContextPreview } from "@/types/docker";
import { dockerErrorText } from "@/utils/dockerMessage";

export default function ImportContexts({
  open,
  onClose,
  onImported,
}: {
  open: boolean;
  onClose: () => void;
  onImported: () => void;
}) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<DockerContextPreview[]>([]);
  const [picked, setPicked] = useState<number[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setError("");
    setPicked([]);
    void dockerContextPreview()
      .then((items) => {
        setRows(items);
        setPicked(items.filter((item) => item.importable && item.current).map((item) => item.index));
      })
      .catch((caught: unknown) => {
        if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
      });
  }, [open, t]);

  async function submit() {
    setBusy(true);
    setError("");
    try {
      await dockerContextImport(picked);
      onImported();
    } catch (caught) {
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={t("zdx.label.docker.importContexts")}
      open={open}
      width={720}
      okText={t("zdx.label.docker.importSelected")}
      cancelText={t("zdx.label.docker.cancel")}
      okButtonProps={{ disabled: picked.length === 0 }}
      confirmLoading={busy}
      onCancel={onClose}
      onOk={() => void submit()}
      destroyOnHidden
    >
      {error && <Alert className="mb-3" type="error" showIcon message={error} />}
      {rows.length === 0 && !error && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.docker.noContexts")} />}
      {rows.length > 0 && (
        <Table
          size="small"
          pagination={false}
          rowKey="index"
          dataSource={rows}
          rowSelection={{
            selectedRowKeys: picked,
            getCheckboxProps: (row) => ({ disabled: !row.importable }),
            onChange: (keys) => setPicked(keys.map(Number)),
          }}
          columns={[
            { title: t("zdx.label.docker.name"), dataIndex: "name", ellipsis: true },
            { title: t("zdx.label.docker.kind"), width: 140, render: (_, row) => kindLabel(t, row.kind) },
            { title: t("zdx.label.docker.host"), dataIndex: "detail", ellipsis: true },
            {
              title: t("zdx.label.docker.status"),
              render: (_, row) => warningText(t, row),
            },
          ]}
        />
      )}
    </Modal>
  );
}

function kindLabel(t: (key: string) => string, kind: string) {
  if (kind === "local") return t("zdx.label.docker.kindLocal");
  if (kind === "ssh") return t("zdx.label.docker.kindSsh");
  if (kind === "tls") return t("zdx.label.docker.kindTls");
  if (kind === "tcp") return t("zdx.label.docker.kindTcp");
  return kind;
}

function warningText(t: (key: string) => string, row: DockerContextPreview) {
  if (row.warning === "unsupported") return t("zdx.text.docker.contextSkipped");
  if (row.warning === "ssh_unmatched") return t("zdx.text.docker.contextSsh");
  if (row.warning === "insecure") return t("zdx.text.docker.contextInsecure");
  if (row.current) return t("zdx.text.docker.contextCurrent");
  return "";
}
