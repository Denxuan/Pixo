import { SshRequestError, sshAnswer, sshHostKeyDecide } from "@/api/ssh";
import type { SshSessionEvent } from "@/types/ssh";
import { sshErrorText } from "@/utils/sshMessage";

export type SshDecision = {
  sessionId: number;
  hostName: string;
  event: Extract<SshSessionEvent, { type: "hostkey" | "prompt" }>;
};

export default function SessionPrompt({
  decision,
  onDone,
}: {
  decision: SshDecision | null;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const [secret, setSecret] = useState("");
  const [remember, setRemember] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const hostkey = decision?.event.type === "hostkey" ? decision.event : null;
  const prompt = decision?.event.type === "prompt" ? decision.event : null;

  useEffect(() => {
    setSecret("");
    setRemember(decision?.event.type === "prompt" ? Boolean(decision.event.remember) : false);
    setError("");
  }, [decision]);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await action();
      onDone();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <Modal
        title={t("zdx.text.ssh.hostkeyTitle", { name: decision?.hostName ?? "" })}
        open={hostkey !== null}
        okText={hostkey?.status === "unknown" ? t("zdx.label.ssh.trust") : t("zdx.label.ssh.confirm")}
        cancelText={t("zdx.label.ssh.cancel")}
        confirmLoading={busy}
        cancelButtonProps={{ style: hostkey?.status === "changed" ? { display: "none" } : undefined }}
        onCancel={() => {
          if (!decision || !hostkey) return;
          if (hostkey.status !== "unknown") {
            onDone();
            return;
          }
          void run(() => sshHostKeyDecide(decision.sessionId, false));
        }}
        onOk={() => {
          if (!decision || hostkey?.status !== "unknown") {
            onDone();
            return;
          }
          void run(() => sshHostKeyDecide(decision.sessionId, true));
        }}
      >
        {hostkey && (
          <Flex vertical gap={8}>
            <div>{hostkey.status === "unknown" ? t("zdx.text.ssh.hostkeyUnknown") : hostkey.revoked ? t("zdx.text.ssh.hostkeyRevoked") : t("zdx.text.ssh.hostkeyChanged")}</div>
            <Typography.Text code>{hostkey.algorithm}</Typography.Text>
            <Typography.Text code>{hostkey.fingerprint}</Typography.Text>
            {hostkey.previous && hostkey.previous !== hostkey.fingerprint && <Typography.Text code>{hostkey.previous}</Typography.Text>}
            {hostkey.source === "user" && <div>{t("zdx.text.ssh.hostkeyUserFile")}</div>}
            {error && <Alert type="error" showIcon message={error} />}
          </Flex>
        )}
      </Modal>
      <Modal
        title={t(prompt?.kind === "passphrase" ? "zdx.text.ssh.passphrasePrompt" : "zdx.text.ssh.passwordPrompt", { name: decision?.hostName ?? "" })}
        open={prompt !== null}
        okText={t("zdx.label.ssh.confirm")}
        cancelText={t("zdx.label.ssh.cancel")}
        confirmLoading={busy}
        onCancel={() => {
          if (!decision || !prompt) return;
          void run(() => sshAnswer(decision.sessionId, prompt.promptId, "", false, true));
        }}
        onOk={() => {
          if (!decision || !prompt) return;
          void run(() => sshAnswer(decision.sessionId, prompt.promptId, secret, remember, false));
        }}
      >
        <Flex vertical gap={12}>
          <Input.Password
            autoFocus
            aria-label={t("zdx.label.ssh.secret")}
            value={secret}
            onChange={(event) => setSecret(event.target.value)}
            onCopy={(event) => event.preventDefault()}
            onCut={(event) => event.preventDefault()}
            onPressEnter={() => {
              if (!decision || !prompt) return;
              void run(() => sshAnswer(decision.sessionId, prompt.promptId, secret, remember, false));
            }}
          />
          {prompt?.canRemember && (
            <Checkbox checked={remember} onChange={(event) => setRemember(event.target.checked)}>
              {t(prompt.kind === "passphrase" ? "zdx.label.ssh.rememberPassphrase" : "zdx.label.ssh.rememberPassword")}
            </Checkbox>
          )}
          {prompt?.canRemember && <Typography.Text type="secondary">{t("zdx.text.ssh.rememberHint")}</Typography.Text>}
          {error && <Alert type="error" showIcon message={error} />}
        </Flex>
      </Modal>
    </>
  );
}
