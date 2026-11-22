import { DockerRequestError, dockerPickFile, dockerSaveEndpoint, dockerSockets } from "@/api/docker";
import type { SshHost } from "@/types/ssh";
import type { DockerEndpoint, DockerEndpointDraft, DockerKind, DockerSocket } from "@/types/docker";
import { dockerErrorText } from "@/utils/dockerMessage";

const REMOTE_SOCKETS = ["/var/run/docker.sock", "/run/podman/podman.sock", "/run/user/1000/podman/podman.sock"];

type FormValues = {
  name: string;
  kind: DockerKind;
  socketPath?: string;
  sshHostId?: number;
  remoteSocket?: string;
  host?: string;
  port?: number;
  caPath?: string;
  certPath?: string;
  keyPath?: string;
  insecure?: boolean;
};

export default function EndpointForm({
  open,
  endpoint,
  sshHosts,
  sshEnabled,
  onClose,
  onSaved,
}: {
  open: boolean;
  endpoint: DockerEndpoint | null;
  sshHosts: SshHost[];
  sshEnabled: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const [form] = Form.useForm<FormValues>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [sockets, setSockets] = useState<DockerSocket[]>([]);
  const kind = Form.useWatch("kind", form) ?? "local";

  useEffect(() => {
    if (!open) return;
    setError("");
    form.setFieldsValue({
      name: endpoint?.name ?? "",
      kind: endpoint?.kind ?? "local",
      socketPath: endpoint?.socketPath ?? "",
      sshHostId: endpoint?.sshHostId ?? undefined,
      remoteSocket: endpoint?.remoteSocket ?? "/var/run/docker.sock",
      host: endpoint?.host ?? "",
      port: endpoint?.port ?? (endpoint?.kind === "tcp" ? 2375 : 2376),
      caPath: endpoint?.caPath ?? "",
      certPath: endpoint?.certPath ?? "",
      keyPath: endpoint?.keyPath ?? "",
      insecure: endpoint?.insecure ?? false,
    });
    void dockerSockets().then(setSockets).catch(() => setSockets([]));
  }, [endpoint, form, open]);

  async function choose(field: "caPath" | "certPath" | "keyPath", title: string) {
    try {
      const path = await dockerPickFile(title);
      if (path) form.setFieldValue(field, path);
    } catch (caught) {
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    }
  }

  async function save() {
    const values = await form.validateFields();
    const draft: DockerEndpointDraft = {
      id: endpoint?.id,
      name: values.name,
      kind: values.kind,
    };
    if (values.kind === "local") draft.socketPath = values.socketPath || undefined;
    if (values.kind === "ssh") {
      draft.sshHostId = values.sshHostId;
      draft.remoteSocket = values.remoteSocket || "/var/run/docker.sock";
    }
    if (values.kind === "tls" || values.kind === "tcp") {
      draft.host = values.host;
      draft.port = values.port;
    }
    if (values.kind === "tls") {
      draft.caPath = values.caPath;
      draft.certPath = values.certPath;
      draft.keyPath = values.keyPath;
    }
    if (values.kind === "tcp") draft.insecure = Boolean(values.insecure);
    setBusy(true);
    setError("");
    try {
      await dockerSaveEndpoint(draft);
      onSaved();
    } catch (caught) {
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={t(endpoint ? "zdx.label.docker.editEndpoint" : "zdx.label.docker.newEndpoint")}
      open={open}
      okText={t("zdx.label.docker.save")}
      cancelText={t("zdx.label.docker.cancel")}
      confirmLoading={busy}
      onCancel={onClose}
      onOk={() => void save()}
      destroyOnHidden
    >
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item name="name" label={t("zdx.label.docker.name")}>
          <Input placeholder={t("zdx.placeholder.docker.name")} />
        </Form.Item>
        <Form.Item name="kind" label={t("zdx.label.docker.kind")}>
          <Segmented
            options={[
              { label: t("zdx.label.docker.kindLocal"), value: "local" },
              { label: t("zdx.label.docker.kindSsh"), value: "ssh", disabled: !sshEnabled && kind !== "ssh" },
              { label: t("zdx.label.docker.kindTls"), value: "tls" },
              { label: t("zdx.label.docker.kindTcp"), value: "tcp" },
            ]}
          />
        </Form.Item>
        {kind === "local" && (
          <Form.Item name="socketPath" label={t("zdx.label.docker.socket")} extra={t("zdx.text.docker.socketHelp")}>
            <AutoComplete
              options={[{ value: "", label: t("zdx.text.docker.socketAuto") }, ...sockets.map((item) => ({ value: item.path, label: `${item.hint} ${item.path}` }))]}
              placeholder={t("zdx.placeholder.docker.socket")}
            />
          </Form.Item>
        )}
        {kind === "ssh" && (
          <>
            {!sshEnabled && <Alert type="warning" showIcon message={t("zdx.text.docker.sshOff")} />}
            {sshHosts.length === 0 && <Alert type="info" showIcon message={t("zdx.text.docker.sshRequired")} />}
            <Form.Item name="sshHostId" label={t("zdx.label.docker.sshHost")} rules={[{ required: true }]}>
              <Select
                options={sshHosts.map((host) => ({ value: host.id, label: `${host.name} (${host.username}@${host.host})` }))}
                disabled={!sshEnabled}
              />
            </Form.Item>
            <Form.Item name="remoteSocket" label={t("zdx.label.docker.remoteSocket")} extra={t("zdx.text.docker.remoteSocketHelp")}>
              <AutoComplete options={REMOTE_SOCKETS.map((value) => ({ value }))} placeholder={t("zdx.placeholder.docker.remoteSocket")} />
            </Form.Item>
          </>
        )}
        {(kind === "tls" || kind === "tcp") && (
          <>
            <Form.Item name="host" label={t("zdx.label.docker.host")} rules={[{ required: true }]}>
              <Input placeholder={t("zdx.placeholder.docker.host")} />
            </Form.Item>
            <Form.Item name="port" label={t("zdx.label.docker.port")}>
              <InputNumber min={1} max={65535} className="w-full" />
            </Form.Item>
          </>
        )}
        {kind === "tls" && (
          <>
            <FileField name="caPath" label={t("zdx.label.docker.ca")} onPick={() => void choose("caPath", t("zdx.label.docker.ca"))} />
            <FileField name="certPath" label={t("zdx.label.docker.cert")} onPick={() => void choose("certPath", t("zdx.label.docker.cert"))} />
            <FileField name="keyPath" label={t("zdx.label.docker.key")} onPick={() => void choose("keyPath", t("zdx.label.docker.key"))} />
          </>
        )}
        {kind === "tcp" && (
          <Form.Item name="insecure" valuePropName="checked" extra={t("zdx.text.docker.insecureHelp")} rules={[{ validator: (_, value) => value ? Promise.resolve() : Promise.reject(new Error(t("zdx.text.docker.error.insecure_required"))) }]}>
            <Checkbox>{t("zdx.label.docker.insecure")}</Checkbox>
          </Form.Item>
        )}
        {error && <Alert type="error" showIcon message={error} />}
      </Form>
    </Modal>
  );
}

function FileField({ name, label, onPick }: { name: string; label: string; onPick: () => void }) {
  const { t } = useTranslation();
  return (
    <Form.Item label={label} required>
      <Space.Compact className="w-full">
        <Form.Item name={name} noStyle rules={[{ required: true }]}>
          <Input readOnly />
        </Form.Item>
        <Button onClick={onPick}>{t("zdx.label.docker.chooseFile")}</Button>
      </Space.Compact>
    </Form.Item>
  );
}
