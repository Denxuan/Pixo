import { SshRequestError, sshDeleteSnippet, sshForwardClose, sshForwardOpen, sshForwards, sshSaveSnippet, sshSnippets } from "@/api/ssh";
import type { SshForward, SshSnippet } from "@/types/ssh";
import { sshErrorText } from "@/utils/sshMessage";

export default function SessionExtras({
  sessionId,
  ready,
  onSend,
}: {
  sessionId: number;
  ready: boolean;
  onSend: (body: string) => void;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<"forward" | "snippet" | null>(null);
  const [forwards, setForwards] = useState<SshForward[]>([]);
  const [snippets, setSnippets] = useState<SshSnippet[]>([]);
  const [kind, setKind] = useState<"local" | "remote" | "dynamic">("local");
  const [listenPort, setListenPort] = useState<number | null>(8080);
  const [targetHost, setTargetHost] = useState("127.0.0.1");
  const [targetPort, setTargetPort] = useState<number | null>(80);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState("");

  const loadForwards = useCallback(async () => {
    if (!sessionId) {
      setForwards([]);
      return;
    }
    try {
      setForwards(await sshForwards(sessionId));
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }, [sessionId, t]);

  const loadSnippets = useCallback(async () => {
    try {
      setSnippets(await sshSnippets());
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }, [t]);

  useEffect(() => {
    if (mode === "forward") void loadForwards();
    if (mode === "snippet") void loadSnippets();
  }, [loadForwards, loadSnippets, mode]);

  async function addForward() {
    if (!sessionId || !listenPort) return;
    if (kind !== "dynamic" && (!targetPort || targetHost.trim().length === 0)) return;
    setError("");
    try {
      await sshForwardOpen(sessionId, kind, listenPort, kind === "dynamic" ? "" : targetHost.trim(), kind === "dynamic" ? 0 : targetPort ?? 0);
      await loadForwards();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  async function stopForward(id: number) {
    setError("");
    try {
      await sshForwardClose(sessionId, id);
      await loadForwards();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  async function saveSnippet() {
    setError("");
    try {
      await sshSaveSnippet({ title, body });
      setTitle("");
      setBody("");
      await loadSnippets();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  async function removeSnippet(id: number) {
    setError("");
    try {
      await sshDeleteSnippet(id);
      await loadSnippets();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  return (
    <>
      <Tooltip title={t("zdx.label.ssh.forwards")}>
        <Button size="small" aria-label={t("zdx.label.ssh.forwards")} icon={<ApiOutlined />} disabled={!ready} onClick={() => { setError(""); setMode("forward"); }} />
      </Tooltip>
      <Tooltip title={t("zdx.label.ssh.snippets")}>
        <Button size="small" aria-label={t("zdx.label.ssh.snippets")} icon={<CodeOutlined />} onClick={() => { setError(""); setMode("snippet"); }} />
      </Tooltip>
      <Drawer
        title={t(mode === "snippet" ? "zdx.label.ssh.snippets" : "zdx.label.ssh.forwards")}
        open={mode !== null}
        onClose={() => setMode(null)}
        size="default"
      >
        {error && <Alert className="mb-3" type="error" showIcon message={error} />}
        {mode === "forward" && (
          <Flex vertical gap={12}>
            <Typography.Paragraph type="secondary">{t("zdx.text.ssh.forwardHelp")}</Typography.Paragraph>
            <Segmented
              aria-label={t("zdx.label.ssh.forwards")}
              value={kind}
              onChange={(value) => {
                if (value === "local" || value === "remote" || value === "dynamic") setKind(value);
              }}
              options={[
                { value: "local", label: t("zdx.label.ssh.forwardLocal") },
                { value: "remote", label: t("zdx.label.ssh.forwardRemote") },
                { value: "dynamic", label: t("zdx.label.ssh.forwardDynamic") },
              ]}
            />
            <Typography.Paragraph type="secondary">
              {t(kind === "remote" ? "zdx.text.ssh.forwardRemoteHelp" : kind === "dynamic" ? "zdx.text.ssh.forwardDynamicHelp" : "zdx.text.ssh.forwardLocalHelp")}
            </Typography.Paragraph>
            <InputNumber
              aria-label={t(kind === "remote" ? "zdx.label.ssh.remotePort" : "zdx.label.ssh.localPort")}
              min={1}
              max={65535}
              value={listenPort}
              placeholder={t(kind === "remote" ? "zdx.placeholder.ssh.remotePort" : "zdx.placeholder.ssh.localPort")}
              onChange={(value) => setListenPort(typeof value === "number" ? value : null)}
            />
            {kind !== "dynamic" && (
              <>
                <Input
                  aria-label={t(kind === "remote" ? "zdx.label.ssh.localForwardHost" : "zdx.label.ssh.remoteHost")}
                  value={targetHost}
                  placeholder={t(kind === "remote" ? "zdx.placeholder.ssh.localForwardHost" : "zdx.placeholder.ssh.remoteHost")}
                  onChange={(event) => setTargetHost(event.target.value)}
                />
                <InputNumber
                  aria-label={t(kind === "remote" ? "zdx.label.ssh.localPort" : "zdx.label.ssh.remotePort")}
                  min={1}
                  max={65535}
                  value={targetPort}
                  placeholder={t(kind === "remote" ? "zdx.placeholder.ssh.localPort" : "zdx.placeholder.ssh.remotePort")}
                  onChange={(value) => setTargetPort(typeof value === "number" ? value : null)}
                />
              </>
            )}
            <Button type="primary" disabled={!ready || !listenPort || (kind !== "dynamic" && (!targetPort || targetHost.trim().length === 0))} onClick={() => void addForward()}>{t("zdx.label.ssh.forwardAdd")}</Button>
            {forwards.length === 0 && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.ssh.forwardEmpty")} />}
            <List
              dataSource={forwards}
              rowKey="id"
              renderItem={(item) => (
                <List.Item actions={[<Button key="stop" type="link" danger onClick={() => void stopForward(item.id)}>{t("zdx.label.ssh.forwardClose")}</Button>]}>
                  <span>{forwardLabel(t, item)}</span>
                </List.Item>
              )}
            />
          </Flex>
        )}
        {mode === "snippet" && (
          <Flex vertical gap={12}>
            <Typography.Paragraph type="secondary">{t("zdx.text.ssh.snippetHelp")}</Typography.Paragraph>
            <Input aria-label={t("zdx.label.ssh.snippetTitle")} value={title} placeholder={t("zdx.placeholder.ssh.snippetTitle")} onChange={(event) => setTitle(event.target.value)} />
            <Input.TextArea aria-label={t("zdx.label.ssh.snippetBody")} value={body} placeholder={t("zdx.placeholder.ssh.snippetBody")} autoSize={{ minRows: 3, maxRows: 8 }} onChange={(event) => setBody(event.target.value)} />
            <Button type="primary" disabled={title.trim().length === 0 || body.trim().length === 0} onClick={() => void saveSnippet()}>{t("zdx.label.ssh.snippetAdd")}</Button>
            {snippets.length === 0 && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.ssh.snippetEmpty")} />}
            <List
              dataSource={snippets}
              rowKey="id"
              renderItem={(item) => (
                <List.Item
                  actions={[
                    <Button key="send" type="link" disabled={!ready} onClick={() => onSend(item.body)}>{t("zdx.label.ssh.snippetSend")}</Button>,
                    <Button key="delete" type="link" danger onClick={() => void removeSnippet(item.id)}>{t("zdx.label.ssh.snippetDelete")}</Button>,
                  ]}
                >
                  <List.Item.Meta title={item.title} description={<span className="whitespace-pre-wrap">{item.body}</span>} />
                </List.Item>
              )}
            />
          </Flex>
        )}
      </Drawer>
    </>
  );
}

function forwardLabel(t: (key: string, options?: Record<string, unknown>) => string, item: SshForward) {
  if (item.kind === "dynamic") return t("zdx.text.ssh.forwardDynamicRow", { port: item.listenPort });
  if (item.kind === "remote") return t("zdx.text.ssh.forwardRemoteRow", { listen: item.listenPort, host: item.targetHost, port: item.targetPort });
  return t("zdx.text.ssh.forwardLocalRow", { listen: item.listenPort, host: item.targetHost, port: item.targetPort });
}
