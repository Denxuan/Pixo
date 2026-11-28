import { SshRequestError, sshForgetKnownHost, sshKnownHosts, sshSetKeepalive, sshStatus } from "@/api/ssh";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import type { SshStatus, TrustedHost } from "@/types/ssh";
import { isDesktop } from "@/utils/desktop";
import { sshErrorText } from "@/utils/sshMessage";

export default function SshSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [status, setStatus] = useState<SshStatus | null>(null);
  const [keepalive, setKeepalive] = useState<number | null>(30);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void sshStatus().then((current) => {
      if (!alive) return;
      setStatus(current);
      setKeepalive(current.keepaliveSecs);
    }).catch((caught: unknown) => {
      if (alive && caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    });
    return () => { alive = false; };
  }, [desktop, t]);

  async function save(seconds: number) {
    setKeepalive(seconds);
    try {
      const next = await sshSetKeepalive(seconds);
      setStatus(next);
      setKeepalive(next.keepaliveSecs);
      setError("");
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  return (
    <Flex vertical gap={24}>
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.ssh.desktopOnly")} />}
      {error && <Alert type="error" showIcon message={error} />}
      <SettingGroup title={t("zdx.label.ssh.behavior")}>
        <SettingRow label={t("zdx.label.ssh.keepalive")} description={t("zdx.text.ssh.keepaliveHint")}>
          <InputNumber
            min={0}
            max={600}
            aria-label={t("zdx.label.ssh.keepalive")}
            value={keepalive}
            disabled={!desktop || !status}
            onChange={(value) => {
              if (typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 600) void save(value);
            }}
          />
        </SettingRow>
        <SettingRow label={t("zdx.label.ssh.authKey")} description={status?.keyringAvailable ? t("zdx.text.system.keyringOn") : t("zdx.text.system.keyringOff")}>
          <Typography.Text type="secondary">{status?.agentAvailable ? t("zdx.text.system.agentOn") : t("zdx.text.system.agentOff")}</Typography.Text>
        </SettingRow>
      </SettingGroup>
      <KnownHosts desktop={desktop} />
    </Flex>
  );
}

function KnownHosts({ desktop }: { desktop: boolean }) {
  const { t } = useTranslation();
  const [hosts, setHosts] = useState<TrustedHost[]>([]);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    if (!desktop) return;
    try {
      setHosts(await sshKnownHosts());
      setError("");
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }, [desktop, t]);

  useEffect(() => {
    void load();
  }, [load]);

  async function forget(host: TrustedHost) {
    setError("");
    try {
      await sshForgetKnownHost(host.host, host.port);
      await load();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  return (
    <SettingGroup title={t("zdx.label.ssh.knownHosts")}>
      <Typography.Paragraph type="secondary">{t("zdx.text.ssh.knownHostsHint")}</Typography.Paragraph>
      {error && <Alert type="error" showIcon message={error} />}
      {hosts.length === 0 && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.ssh.knownEmpty")} />}
      {hosts.length > 0 && (
        <List
          dataSource={hosts}
          rowKey={(item) => `${item.host}:${item.port}:${item.fingerprint}`}
          renderItem={(item) => (
            <List.Item actions={[
              <Button key="forget" type="link" danger onClick={() => void forget(item)}>{t("zdx.label.ssh.forgetHost")}</Button>,
            ]}>
              <List.Item.Meta
                title={t("zdx.text.ssh.knownHost", { host: item.host, port: item.port })}
                description={t("zdx.text.ssh.knownFingerprint", { algorithm: item.algorithm, fingerprint: item.fingerprint })}
              />
            </List.Item>
          )}
        />
      )}
    </SettingGroup>
  );
}
