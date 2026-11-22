import { SshRequestError, sshForgetKey, sshPickKey, sshSaveHost } from "@/api/ssh";
import type { SshAuth, SshHost, SshHostDraft } from "@/types/ssh";
import { isSshAuth } from "@/types/ssh";
import { sshErrorText } from "@/utils/sshMessage";

type FormValues = {
  name: string;
  groupName?: string;
  host: string;
  port: number;
  username: string;
  auth: SshAuth;
  keyPath?: string;
  remember?: boolean;
  startDir?: string;
  term?: string;
  jumps?: number[];
  agentForward?: boolean;
};

export default function HostForm({
  host,
  hosts,
  open,
  keyringAvailable,
  agentAvailable,
  onClose,
  onSaved,
}: {
  host: SshHost | null;
  hosts: SshHost[];
  open: boolean;
  keyringAvailable: boolean;
  agentAvailable: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const [form] = Form.useForm<FormValues>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const auth = Form.useWatch("auth", form);

  useEffect(() => {
    if (!open) return;
    setError("");
    form.setFieldsValue({
      name: host?.name ?? "",
      groupName: host?.groupName ?? "",
      host: host?.host ?? "",
      port: host?.port ?? 22,
      username: host?.username ?? "",
      auth: host?.auth ?? "password",
      keyPath: host?.keyPath ?? "",
      remember: host?.remember ?? false,
      startDir: host?.startDir ?? "",
      term: host?.term ?? "xterm-256color",
      jumps: host?.jumps ?? [],
      agentForward: host?.agentForward ?? false,
    });
  }, [form, host, open]);

  async function chooseKey() {
    try {
      const path = await sshPickKey(t("zdx.label.ssh.chooseKey"));
      if (path) form.setFieldValue("keyPath", path);
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  async function forget() {
    if (!host) return;
    setBusy(true);
    setError("");
    try {
      await sshForgetKey(host.id);
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    } finally {
      setBusy(false);
    }
  }

  async function submit(values: FormValues) {
    if (!isSshAuth(values.auth)) return;
    const draft: SshHostDraft = {
      id: host?.id,
      name: values.name,
      groupName: values.groupName,
      host: values.host,
      port: values.port,
      username: values.username,
      auth: values.auth,
      keyPath: values.keyPath,
      remember: keyringAvailable ? Boolean(values.remember) : Boolean(host?.remember),
      startDir: values.startDir,
      term: values.term,
      jumps: values.jumps ?? [],
      agentForward: Boolean(values.agentForward),
    };
    setBusy(true);
    setError("");
    try {
      await sshSaveHost(draft);
      onSaved();
      onClose();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={host ? t("zdx.label.ssh.editHost") : t("zdx.label.ssh.newHost")}
      open={open}
      centered
      confirmLoading={busy}
      okText={t("zdx.label.ssh.save")}
      cancelText={t("zdx.label.ssh.cancel")}
      destroyOnHidden
      styles={{
        wrapper: { overflow: "hidden" },
        container: { display: "flex", flexDirection: "column", maxHeight: "calc(100vh - 48px)" },
        header: { flexShrink: 0 },
        body: {
          flex: "1 1 auto",
          minHeight: 0,
          overflowY: "auto",
          overscrollBehavior: "contain",
        },
        footer: { flexShrink: 0 },
      }}
      onCancel={onClose}
      onOk={() => form.submit()}
    >
      <Form form={form} layout="vertical" requiredMark={false} onFinish={(values) => void submit(values)}>
        {error && <Alert type="error" showIcon className="mb-3" message={error} />}
        <Form.Item name="name" label={t("zdx.label.ssh.name")} rules={[{ required: true, whitespace: true, message: t("zdx.text.ssh.required") }]}>
          <Input placeholder={t("zdx.placeholder.ssh.name")} />
        </Form.Item>
        <Form.Item name="groupName" label={t("zdx.label.ssh.group")}>
          <Input placeholder={t("zdx.placeholder.ssh.group")} />
        </Form.Item>
        <Flex gap={12}>
          <Form.Item className="min-w-0 flex-1" name="host" label={t("zdx.label.ssh.host")} rules={[{ required: true, whitespace: true, message: t("zdx.text.ssh.required") }]}>
            <Input placeholder={t("zdx.placeholder.ssh.host")} />
          </Form.Item>
          <Form.Item name="port" label={t("zdx.label.ssh.port")} rules={[{ required: true, type: "number", min: 1, max: 65535, message: t("zdx.text.ssh.required") }]}>
            <InputNumber min={1} max={65535} className="w-28" />
          </Form.Item>
        </Flex>
        <Form.Item name="username" label={t("zdx.label.ssh.username")} rules={[{ required: true, whitespace: true, message: t("zdx.text.ssh.required") }]}>
          <Input placeholder={t("zdx.placeholder.ssh.username")} />
        </Form.Item>
        <Form.Item name="auth" label={t("zdx.label.ssh.auth")}>
          <Select
            aria-label={t("zdx.label.ssh.auth")}
            options={[
              { value: "password", label: t("zdx.label.ssh.authPassword") },
              { value: "key", label: t("zdx.label.ssh.authKey") },
              { value: "agent", label: t("zdx.label.ssh.authAgent") },
            ]}
          />
        </Form.Item>
        {auth === "key" && (
          <Form.Item name="keyPath" label={t("zdx.label.ssh.keyPath")} rules={[{ required: true, whitespace: true, message: t("zdx.text.ssh.required") }]}>
            <Input
              readOnly
              placeholder={t("zdx.placeholder.ssh.keyPath")}
              addonAfter={<Button htmlType="button" type="link" size="small" icon={<KeyOutlined />} aria-label={t("zdx.label.ssh.chooseKey")} onClick={() => void chooseKey()}>{t("zdx.label.ssh.chooseKey")}</Button>}
            />
          </Form.Item>
        )}
        {auth === "agent" ? (
          <Typography.Paragraph type="secondary">{t(agentAvailable ? "zdx.text.ssh.agentHint" : "zdx.text.ssh.agentMissing")}</Typography.Paragraph>
        ) : keyringAvailable ? (
          <Form.Item name="remember" valuePropName="checked" extra={t("zdx.text.ssh.rememberHint")}>
            <Checkbox>{t(auth === "key" ? "zdx.label.ssh.rememberPassphrase" : "zdx.label.ssh.rememberPassword")}</Checkbox>
          </Form.Item>
        ) : (
          <Typography.Paragraph type="secondary">{t("zdx.text.ssh.keyringUnavailable")}</Typography.Paragraph>
        )}
        <Form.Item name="agentForward" valuePropName="checked" extra={t("zdx.text.ssh.agentForwardHint")}>
          <Checkbox>{t("zdx.label.ssh.agentForward")}</Checkbox>
        </Form.Item>
        <Form.Item name="jumps" label={t("zdx.label.ssh.jumps")} extra={t("zdx.text.ssh.jumpsHint")}>
          <Select
            mode="multiple"
            allowClear
            aria-label={t("zdx.label.ssh.jumps")}
            placeholder={t("zdx.placeholder.ssh.jumps")}
            options={hosts.filter((item) => item.id !== host?.id).map((item) => ({
              value: item.id,
              label: `${item.name} (${item.username}@${item.host})`,
            }))}
          />
        </Form.Item>
        <Form.Item name="startDir" label={t("zdx.label.ssh.startDir")}>
          <Input placeholder={t("zdx.placeholder.ssh.startDir")} />
        </Form.Item>
        <Form.Item name="term" label={t("zdx.label.ssh.term")}>
          <Input placeholder="xterm-256color" />
        </Form.Item>
        {host && (
          <Button htmlType="button" onClick={() => void forget()} disabled={busy}>{t("zdx.label.ssh.forgetKey")}</Button>
        )}
      </Form>
    </Modal>
  );
}
