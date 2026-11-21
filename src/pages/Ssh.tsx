import { SshRequestError, sshDeleteHost, sshHosts, sshSetEnabled, sshStatus } from "@/api/ssh";
import HostForm from "@/components/ssh/HostForm";
import HostList from "@/components/ssh/HostList";
import HostSplit from "@/components/ssh/HostSplit";
import SessionPrompt, { type SshDecision } from "@/components/ssh/SessionPrompt";
import TerminalView from "@/components/ssh/TerminalView";
import Page from "@/components/Page";
import type { SshHost, SshSessionEvent } from "@/types/ssh";
import { isDesktop } from "@/utils/desktop";
import { sshErrorText } from "@/utils/sshMessage";

type OpenTab = {
  key: string;
  host: SshHost;
  generation: number;
  sessionId: number | null;
  phase: string;
};

let tabSerial = 0;

export default function SshPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [keyringAvailable, setKeyringAvailable] = useState(false);
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [editing, setEditing] = useState<SshHost | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [tabs, setTabs] = useState<OpenTab[]>([]);
  const [active, setActive] = useState("");
  const [decision, setDecision] = useState<SshDecision | null>(null);
  const [error, setError] = useState("");

  const loadHosts = useCallback(async () => {
    try {
      setHosts(await sshHosts());
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }, [t]);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void sshStatus().then((status) => {
      if (!alive) return;
      setEnabled(status.enabled);
      setKeyringAvailable(status.keyringAvailable);
      if (status.enabled) void loadHosts();
    }).catch((caught) => {
      if (alive && caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    });
    return () => { alive = false; };
  }, [desktop, loadHosts, t]);

  function enable() {
    Modal.confirm({
      title: t("zdx.text.ssh.enableTitle"),
      content: t("zdx.text.ssh.enableBody"),
      okText: t("zdx.label.ssh.enable"),
      cancelText: t("zdx.label.ssh.cancel"),
      onOk: async () => {
        const status = await sshSetEnabled(true);
        setEnabled(status.enabled);
        setKeyringAvailable(status.keyringAvailable);
        await loadHosts();
      },
    });
  }

  function connect(host: SshHost) {
    setError("");
    const key = `ssh-${host.id}-${tabSerial += 1}`;
    setTabs((current) => [...current, { key, host, generation: 0, sessionId: null, phase: "connecting" }]);
    setActive(key);
    setSelectedId(host.id);
  }

  async function removeHost(host: SshHost) {
    try {
      await sshDeleteHost(host.id);
      setTabs((current) => current.filter((tab) => tab.host.id !== host.id));
      await loadHosts();
    } catch (caught) {
      if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
    }
  }

  function onEvent(tab: OpenTab, sessionId: number, event: SshSessionEvent) {
    if (event.type === "state") {
      setTabs((current) => current.map((item) => item.key === tab.key ? { ...item, phase: event.state, sessionId } : item));
      if (event.state === "ready") void loadHosts();
    }
    if (event.type === "closed") {
      setTabs((current) => current.map((item) => item.key === tab.key ? { ...item, phase: "closed", sessionId } : item));
      setDecision((current) => {
        if (current?.sessionId !== sessionId) return current;
        if (current.event.type === "hostkey" && current.event.status !== "unknown") return current;
        return null;
      });
    }
    if (event.type === "hostkey" || event.type === "prompt") {
      setDecision({ sessionId, hostName: tab.host.name, event });
    }
    if (event.type === "notice") {
      setError(sshErrorText(t, { code: event.code, params: event.params ?? {} }));
      void loadHosts();
    }
  }

  if (!desktop) {
    return <Page title={t("zdx.label.ssh.title")}><Alert type="info" showIcon message={t("zdx.text.ssh.desktopOnly")} /></Page>;
  }
  if (enabled === null) {
    return <Page title={t("zdx.label.ssh.title")}><Spin /></Page>;
  }
  if (!enabled) {
    return (
      <Page title={t("zdx.label.ssh.title")} description={t("zdx.text.ssh.description")} extra={<Button type="primary" onClick={enable}>{t("zdx.label.ssh.enable")}</Button>}>
        {error && <Alert type="error" showIcon message={error} />}
        <Alert type="info" showIcon message={t("zdx.text.ssh.disabled")} />
      </Page>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-1">
      <HostSplit
        start={(
          <HostList
            hosts={hosts}
            query={query}
            selectedId={selectedId}
            onQuery={setQuery}
            onSelect={setSelectedId}
            onConnect={connect}
            onEdit={(host) => { setEditing(host); setFormOpen(true); }}
            onDelete={(host) => void removeHost(host)}
            onCreate={() => { setEditing(null); setFormOpen(true); }}
          />
        )}
        end={(
          <>
            {error && <Alert className="m-3" type="error" showIcon message={error} />}
            {tabs.length === 0 ? (
              <div className="flex flex-1 items-center justify-center">
                <Empty description={t("zdx.text.ssh.noSession")} />
              </div>
            ) : (
              <Tabs
                className="min-h-0 flex-1 [&_.ant-tabs-nav]:px-3 [&_.ant-tabs-nav]:pt-2 [&_.ant-tabs-body-holder]:flex [&_.ant-tabs-body-holder]:min-h-0 [&_.ant-tabs-body-holder]:flex-1 [&_.ant-tabs-body-holder]:flex-col"
                classNames={{ body: "min-h-0 flex-1", content: "h-full min-h-0" }}
                styles={{
                  root: { display: "flex", flexDirection: "column", flex: 1, minHeight: 0 },
                  body: { height: "100%", minHeight: 0 },
                  content: { height: "100%", minHeight: 0, display: "flex", flexDirection: "column" },
                }}
                type="editable-card"
                hideAdd
                activeKey={active}
                onChange={setActive}
                onEdit={(key, action) => {
                  if (action !== "remove" || typeof key !== "string") return;
                  setTabs((current) => {
                    const next = current.filter((tab) => tab.key !== key);
                    if (active === key) setActive(next.length > 0 ? next[next.length - 1].key : "");
                    return next;
                  });
                }}
                items={tabs.map((tab) => ({
                  key: tab.key,
                  label: (
                    <span className="inline-flex items-center gap-2">
                      <span className={tab.phase === "ready" ? "inline-block h-1.5 w-1.5 rounded-full bg-green-500" : tab.phase === "closed" ? "inline-block h-1.5 w-1.5 rounded-full bg-red-400" : "inline-block h-1.5 w-1.5 rounded-full bg-amber-400"} />
                      {tab.host.name}
                    </span>
                  ),
                  children: (
                    <TerminalView
                      host={tab.host}
                      connectKey={tab.generation}
                      onSession={(sessionId) => setTabs((current) => current.map((item) => item.key === tab.key ? { ...item, sessionId } : item))}
                      onEvent={(sessionId, event) => onEvent(tab, sessionId, event)}
                      onReconnect={() => setTabs((current) => current.map((item) => item.key === tab.key ? { ...item, generation: item.generation + 1, phase: "connecting", sessionId: null } : item))}
                    />
                  ),
                }))}
              />
            )}
          </>
        )}
      />
      <HostForm
        host={editing}
        open={formOpen}
        keyringAvailable={keyringAvailable}
        onClose={() => setFormOpen(false)}
        onSaved={() => void loadHosts()}
      />
      <SessionPrompt decision={decision} onDone={() => setDecision(null)} />
    </div>
  );
}
