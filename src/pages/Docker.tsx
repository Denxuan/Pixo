import { Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { DockerRequestError, dockerAnswer, dockerConnect, dockerDeleteEndpoint, dockerDisconnect, dockerEndpoints, dockerHostKeyDecide, dockerSetEnabled, dockerStatus } from "@/api/docker";
import { SshRequestError, sshHosts, sshStatus } from "@/api/ssh";
import EndpointForm from "@/components/docker/EndpointForm";
import EndpointList from "@/components/docker/EndpointList";
import ImportContexts from "@/components/docker/ImportContexts";
import ResourcePanel from "@/components/docker/ResourcePanel";
import Page from "@/components/Page";
import HostSplit from "@/components/ssh/HostSplit";
import SessionPrompt, { type SshDecision } from "@/components/ssh/SessionPrompt";
import type { DockerConnectEvent, DockerEndpoint, EngineSummary } from "@/types/docker";
import type { SshHost, SshSessionEvent } from "@/types/ssh";
import { isDesktop } from "@/utils/desktop";
import { dockerErrorText } from "@/utils/dockerMessage";

export default function DockerPage() {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [endpoints, setEndpoints] = useState<DockerEndpoint[]>([]);
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [sshEnabled, setSshEnabled] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [phase, setPhase] = useState("");
  const [summary, setSummary] = useState<EngineSummary | null>(null);
  const [error, setError] = useState("");
  const [decision, setDecision] = useState<SshDecision | null>(null);
  const [editing, setEditing] = useState<DockerEndpoint | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const ticket = useRef(0);
  const selected = endpoints.find((item) => item.id === selectedId) ?? null;

  const reload = useCallback(async () => {
    const [status, listed] = await Promise.all([dockerStatus(), dockerEndpoints()]);
    setEnabled(status.enabled);
    setEndpoints(listed);
    try {
      const ssh = await sshStatus();
      setSshEnabled(ssh.enabled);
      setHosts(await sshHosts());
    } catch {
      setSshEnabled(false);
      setHosts([]);
    }
  }, []);

  useEffect(() => {
    if (!isDesktop()) {
      setEnabled(false);
      return;
    }
    void reload().catch((caught: unknown) => {
      setEnabled((current) => current ?? false);
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    });
  }, [reload, t]);

  useEffect(() => {
    if (!isDesktop()) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<boolean>("docker://enabled", (event) => {
      setEnabled(event.payload);
      if (!event.payload) {
        ticket.current += 1;
        setPhase("");
        setSummary(null);
        setSelectedId(null);
        setDecision(null);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  function changeEnabled(next: boolean) {
    Modal.confirm({
      title: t(next ? "zdx.text.docker.enableTitle" : "zdx.text.docker.disableTitle"),
      content: t(next ? "zdx.text.docker.enableBody" : "zdx.text.docker.disableBody"),
      okText: t(next ? "zdx.label.docker.enable" : "zdx.label.docker.confirm"),
      cancelText: t("zdx.label.docker.cancel"),
      onOk: async () => {
        setBusy(true);
        setError("");
        try {
          const status = await dockerSetEnabled(next);
          setEnabled(status.enabled);
          if (status.enabled) await reload();
        } catch (caught) {
          if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
        } finally {
          setBusy(false);
        }
      },
    });
  }

  const selectedRef = useRef<number | null>(null);
  selectedRef.current = selectedId;
  useEffect(() => () => {
    ticket.current += 1;
    const id = selectedRef.current;
    if (id) void dockerDisconnect(id).catch(() => undefined);
  }, []);

  async function connect(endpoint: DockerEndpoint) {
    const current = ++ticket.current;
    setSelectedId(endpoint.id);
    setSummary(null);
    setDecision(null);
    setError("");
    if (endpoint.kind === "ssh" && !sshEnabled) {
      setPhase("closed");
      setError(t("zdx.text.docker.sshOff"));
      return;
    }
    setPhase("connecting");
    const previous = selectedId;
    if (previous && previous !== endpoint.id) void dockerDisconnect(previous).catch(() => undefined);
    const events = new Channel<DockerConnectEvent>();
    const pending: DockerConnectEvent[] = [];
    const gate = { id: 0 };
    const apply = (event: DockerConnectEvent) => {
      if (current !== ticket.current) return;
      if (event.type === "state") setPhase("connecting");
      if (event.type === "ready") {
        setPhase("ready");
        setSummary(event.summary);
        setError("");
      }
      if (event.type === "closed") {
        setPhase("closed");
        setSummary(null);
        if (event.code !== "cancelled") setError(dockerErrorText(t, { code: event.code, params: event.params ?? {} }));
      }
      if (event.type === "hostkey" || event.type === "prompt") {
        const host = hosts.find((item) => item.id === endpoint.sshHostId);
        setDecision({ sessionId: gate.id, hostName: host?.name || endpoint.name, event: event as Extract<SshSessionEvent, { type: "hostkey" | "prompt" }> });
      }
    };
    events.onmessage = (event) => {
      if (!gate.id) pending.push(event);
      else apply(event);
    };
    try {
      gate.id = await dockerConnect(endpoint.id, events);
      if (current !== ticket.current) {
        void dockerDisconnect(endpoint.id).catch(() => undefined);
        return;
      }
      for (const event of pending.splice(0)) apply(event);
    } catch (caught) {
      if (current !== ticket.current) return;
      setPhase("closed");
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    }
  }

  function remove(endpoint: DockerEndpoint) {
    Modal.confirm({
      title: t("zdx.text.docker.deleteConfirm", { name: endpoint.name }),
      okText: t("zdx.label.docker.deleteEndpoint"),
      cancelText: t("zdx.label.docker.cancel"),
      okButtonProps: { danger: true },
      onOk: async () => {
        await dockerDeleteEndpoint(endpoint.id);
        if (selectedId === endpoint.id) {
          ticket.current += 1;
          setSelectedId(null);
          setPhase("");
          setSummary(null);
        }
        await reload();
      },
    });
  }

  if (!isDesktop()) {
    return <Page title={t("zdx.label.docker.title")} description={t("zdx.text.docker.desktopOnly")}><Alert type="info" showIcon message={t("zdx.text.docker.desktopOnly")} /></Page>;
  }
  if (enabled === null) {
    return <div className="flex h-full items-center justify-center"><Spin /></div>;
  }
  if (enabled === false) {
    return (
      <Page
        title={t("zdx.label.docker.title")}
        description={t("zdx.text.docker.description")}
        extra={<FeatureSwitch enabled={false} busy={busy} onChange={changeEnabled} />}
      >
        {error && <Alert className="mb-4" type="error" showIcon message={error} />}
        <Empty description={t("zdx.text.docker.disabled")} />
      </Page>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-1 flex-col">
      <div className="flex shrink-0 items-center justify-between px-4 py-3" style={{ borderBottom: `1px solid ${token.colorBorderSecondary}` }}>
        <span className="font-medium" style={{ color: token.colorText }}>{t("zdx.label.docker.title")}</span>
        <FeatureSwitch enabled busy={busy} onChange={changeEnabled} />
      </div>
      <div className="flex min-h-0 flex-1">
      <HostSplit
        storageKey="pixo-docker-endpoint-pane"
        labelKey="zdx.label.docker.resizeEndpoints"
        start={(
          <EndpointList
            endpoints={endpoints}
            selectedId={selectedId}
            sshEnabled={sshEnabled}
            phase={phase}
            onSelect={(endpoint) => void connect(endpoint)}
            onCreate={() => { setEditing(null); setFormOpen(true); }}
            onEdit={(endpoint) => { setEditing(endpoint); setFormOpen(true); }}
            onDelete={remove}
            onImport={() => setImportOpen(true)}
          />
        )}
        end={(
      <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto">
        {!selected && <div className="flex flex-1 items-center justify-center"><Empty description={t("zdx.text.docker.selectEndpoint")} /></div>}
        {selected && phase === "connecting" && (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <Spin />
            <span>{t("zdx.text.docker.connecting")}</span>
          </div>
        )}
        {selected && phase === "closed" && (
          <div className="flex flex-1 items-center justify-center p-6">
            <Empty description={error || t("zdx.text.docker.unreachableTitle")}>
              <Button onClick={() => void connect(selected)}>{t("zdx.label.docker.connect")}</Button>
            </Empty>
          </div>
        )}
        {selected && phase === "ready" && summary && <ResourcePanel endpointId={selected.id} summary={summary} />}
      </section>
        )}
      />
      </div>
      <EndpointForm
        open={formOpen}
        endpoint={editing}
        sshHosts={hosts}
        sshEnabled={sshEnabled}
        onClose={() => setFormOpen(false)}
        onSaved={() => { setFormOpen(false); void reload(); }}
      />
      <ImportContexts open={importOpen} onClose={() => setImportOpen(false)} onImported={() => { setImportOpen(false); void reload(); }} />
      <SessionPrompt
        decision={decision}
        onDone={() => setDecision(null)}
        onHostKey={async (sessionId, accept) => {
          try {
            await dockerHostKeyDecide(sessionId, accept);
          } catch (caught) {
            if (caught instanceof DockerRequestError) throw new SshRequestError(caught.code, caught.params);
            throw caught;
          }
        }}
        onAnswer={async (sessionId, promptId, secret, remember, cancel) => {
          try {
            await dockerAnswer(sessionId, promptId, secret, remember, cancel);
          } catch (caught) {
            if (caught instanceof DockerRequestError) throw new SshRequestError(caught.code, caught.params);
            throw caught;
          }
        }}
      />
    </div>
  );
}

function FeatureSwitch({
  enabled,
  busy,
  onChange,
}: {
  enabled: boolean;
  busy: boolean;
  onChange: (enabled: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <span className="flex items-center gap-2">
      <Typography.Text>{t("zdx.label.docker.feature")}</Typography.Text>
      <Switch aria-label={t("zdx.label.docker.feature")} checked={enabled} disabled={busy} onChange={onChange} />
    </span>
  );
}
