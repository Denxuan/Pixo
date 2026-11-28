import { Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { DockerRequestError, dockerAnswer, dockerConnect, dockerDeleteEndpoint, dockerDisconnect, dockerEndpoints, dockerHostKeyDecide, dockerRenew, dockerStatus } from "@/api/docker";
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
  const [enabled, setEnabled] = useState<boolean | null>(null);
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
  const launchedEndpoint = useRef<number | null>(null);
  const phaseRef = useRef("");
  const reviving = useRef(false);
  const connectRef = useRef<(endpoint: DockerEndpoint) => Promise<void>>(async () => undefined);
  const location = useLocation();
  const navigate = useNavigate();
  const selected = endpoints.find((item) => item.id === selectedId) ?? null;
  phaseRef.current = phase;

  const reload = useCallback(async () => {
    const [status, listed, ssh] = await Promise.all([
      dockerStatus(),
      dockerEndpoints(),
      sshStatus()
        .then(async (current) => ({ enabled: current.enabled, hosts: current.enabled ? await sshHosts() : [] }))
        .catch(() => ({ enabled: false, hosts: [] as SshHost[] })),
    ]);
    setEnabled(status.enabled);
    setEndpoints(listed);
    setSshEnabled(ssh.enabled);
    setHosts(ssh.hosts);
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

  const selectedRef = useRef<number | null>(null);
  selectedRef.current = selectedId;
  useEffect(() => () => {
    ticket.current += 1;
    const id = selectedRef.current;
    if (id) void dockerDisconnect(id).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (enabled !== true) return;
    const endpointId = (location.state as { hostId?: number } | null)?.hostId;
    if (!endpointId) {
      launchedEndpoint.current = null;
      return;
    }
    if (endpoints.length === 0 || launchedEndpoint.current === endpointId) return;
    const endpoint = endpoints.find((item) => item.id === endpointId);
    if (!endpoint) return;
    launchedEndpoint.current = endpointId;
    void connect(endpoint);
    navigate("/docker", { replace: true, state: null });
  }, [enabled, endpoints, location.state, navigate]);

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

  connectRef.current = connect;

  function revive(endpoint: DockerEndpoint) {
    if (reviving.current || phaseRef.current === "connecting") return;
    reviving.current = true;
    void connectRef.current(endpoint);
  }

  useEffect(() => {
    if (phase !== "connecting") reviving.current = false;
  }, [phase]);

  useEffect(() => {
    if (phase !== "ready" || selectedId === null) return;
    const endpoint = endpoints.find((item) => item.id === selectedId);
    if (!endpoint) return;
    let stopped = false;
    const timer = window.setInterval(() => {
      void dockerRenew(endpoint.id).catch((caught: unknown) => {
        if (stopped) return;
        if (caught instanceof DockerRequestError && (caught.code === "disconnected" || caught.code === "unreachable" || caught.code === "timeout")) {
          revive(endpoint);
        }
      });
    }, 20_000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [phase, selectedId, endpoints]);

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
      >
        {error && <Alert className="mb-4" type="error" showIcon message={error} />}
        <Empty description={t("zdx.text.docker.disabled")} />
      </Page>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-1 flex-col">
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
            onSettings={() => navigate("/settings/docker")}
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
              {selected.kind === "ssh" && !sshEnabled ? (
                <Button onClick={() => navigate("/settings/ssh")}>{t("zdx.label.docker.openSsh")}</Button>
              ) : (
                <Button onClick={() => void connect(selected)}>{t("zdx.label.docker.connect")}</Button>
              )}
            </Empty>
          </div>
        )}
        {selected && phase === "ready" && summary && <ResourcePanel endpointId={selected.id} summary={summary} host={containerHost(selected, hosts)} onLost={() => revive(selected)} />}
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

function containerHost(endpoint: DockerEndpoint, hosts: SshHost[]): string {
  if (endpoint.kind === "ssh") return hosts.find((item) => item.id === endpoint.sshHostId)?.host ?? "";
  return "127.0.0.1";
}
