import { DockerRequestError, dockerSaveEndpoint, dockerSockets } from "@/api/docker";
import { sshStatus } from "@/api/ssh";
import HostForm from "@/components/ssh/HostForm";
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
};

export default function EndpointForm({
  open,
  endpoint,
  sshHosts,
  sshEnabled,
  onClose,
  onSaved,
  onHostSaved,
}: {
  open: boolean;
  endpoint: DockerEndpoint | null;
  sshHosts: SshHost[];
  sshEnabled: boolean;
  onClose: () => void;
  onSaved: () => void;
  onHostSaved: (host: SshHost) => void;
}) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const [form] = Form.useForm<FormValues>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [sockets, setSockets] = useState<DockerSocket[]>([]);
  const [hostOpen, setHostOpen] = useState(false);
  const [hostMenu, setHostMenu] = useState(false);
  const [keyringAvailable, setKeyringAvailable] = useState(false);
  const [agentAvailable, setAgentAvailable] = useState(false);
  const kind = Form.useWatch("kind", form) ?? "local";

  useEffect(() => {
    if (open) return;
    setHostOpen(false);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    setError("");
    const kind = endpoint?.kind === "ssh" ? "ssh" : "local";
    form.setFieldsValue({
      name: endpoint?.name ?? "",
      kind,
      socketPath: endpoint?.socketPath ?? "",
      sshHostId: endpoint?.sshHostId ?? undefined,
      remoteSocket: endpoint?.remoteSocket ?? "/var/run/docker.sock",
    });
    void dockerSockets().then(setSockets).catch(() => setSockets([]));
  }, [endpoint, form, open]);

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

  function openHostForm() {
    void sshStatus().then((status) => {
      setKeyringAvailable(status.keyringAvailable);
      setAgentAvailable(status.agentAvailable);
    }).catch(() => undefined);
    setHostOpen(true);
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
      <Form
        form={form}
        layout="vertical"
        requiredMark={(label, { required }) => (
          <>
            {label}
            {required && <span style={{ color: token.colorError }}> *</span>}
          </>
        )}
      >
        <Form.Item name="name" label={t("zdx.label.docker.name")}>
          <Input placeholder={t("zdx.placeholder.docker.name")} />
        </Form.Item>
        <Form.Item name="kind" label={t("zdx.label.docker.kind")}>
          <Segmented
            options={[
              { label: t("zdx.label.docker.kindLocal"), value: "local" },
              { label: t("zdx.label.docker.kindSsh"), value: "ssh", disabled: !sshEnabled && kind !== "ssh" },
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
            <Form.Item label={t("zdx.label.docker.sshHost")} required>
              <Space.Compact block>
                <Form.Item name="sshHostId" noStyle rules={[{ required: true, message: t("zdx.text.ssh.required") }]}>
                  <Select
                    className="min-w-0 flex-1"
                    showSearch
                    optionFilterProp="label"
                    placeholder={t("zdx.placeholder.docker.sshHost")}
                    disabled={!sshEnabled}
                    open={hostMenu}
                    options={sshHosts.map((host) => ({ value: host.id, label: `${host.name} (${host.username}@${host.host})` }))}
                    onOpenChange={setHostMenu}
                    popupRender={(menu) => (
                      <>
                        {menu}
                        <Button
                          htmlType="button"
                          type="text"
                          block
                          className="mt-1 justify-start"
                          icon={<PlusOutlined />}
                          disabled={!sshEnabled}
                          onMouseDown={(event) => event.preventDefault()}
                          onClick={() => {
                            setHostMenu(false);
                            openHostForm();
                          }}
                        >
                          {t("zdx.label.ssh.newHost")}
                        </Button>
                      </>
                    )}
                  />
                </Form.Item>
                <Button
                  htmlType="button"
                  aria-label={t("zdx.label.ssh.newHost")}
                  icon={<PlusOutlined />}
                  disabled={!sshEnabled}
                  onClick={openHostForm}
                />
              </Space.Compact>
            </Form.Item>
            <Form.Item name="remoteSocket" label={t("zdx.label.docker.remoteSocket")} extra={t("zdx.text.docker.remoteSocketHelp")}>
              <AutoComplete options={REMOTE_SOCKETS.map((value) => ({ value }))} placeholder={t("zdx.placeholder.docker.remoteSocket")} />
            </Form.Item>
          </>
        )}
        {error && <Alert type="error" showIcon message={error} />}
      </Form>
      <HostForm
        host={null}
        hosts={sshHosts}
        open={hostOpen}
        zIndex={1200}
        keyringAvailable={keyringAvailable}
        agentAvailable={agentAvailable}
        onClose={() => setHostOpen(false)}
        onSaved={(host) => {
          onHostSaved(host);
          form.setFieldValue("sshHostId", host.id);
        }}
      />
    </Modal>
  );
}
