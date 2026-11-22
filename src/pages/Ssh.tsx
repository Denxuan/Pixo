import { SshRequestError, sshAnswer, sshDeleteHost, sshDisconnect, sshHostKeyDecide, sshHosts, sshSetEnabled, sshSetKeepalive, sshStatus } from "@/api/ssh";
import HostForm from "@/components/ssh/HostForm";
import HostList from "@/components/ssh/HostList";
import HostSplit from "@/components/ssh/HostSplit";
import ImportConfig from "@/components/ssh/ImportConfig";
import SessionPrompt, { type SshDecision } from "@/components/ssh/SessionPrompt";
import TerminalView from "@/components/ssh/TerminalView";
import Page from "@/components/Page";
import type { SshHost, SshSessionEvent } from "@/types/ssh";
import { isDesktop } from "@/utils/desktop";
import { sshErrorText } from "@/utils/sshMessage";
import { focusExistingSession, sshEndpoint, tabsAfterClose } from "@/utils/sshTabs";
import { readOpenHosts, rememberHost, writeOpenHosts } from "@/utils/sshSessions";

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
  const [agentAvailable, setAgentAvailable] = useState(false);
  const [keepalive, setKeepalive] = useState(30);
  const [importOpen, setImportOpen] = useState(false);
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [editing, setEditing] = useState<SshHost | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [tabs, setTabs] = useState<OpenTab[]>([]);
  const [active, setActive] = useState("");
  const [tabMenu, setTabMenu] = useState<{ key: string; x: number; y: number } | null>(null);
  const [decision, setDecision] = useState<SshDecision | null>(null);
  const [error, setError] = useState("");
  const tabsRef = useRef(tabs);
  const activeRef = useRef(active);
  const tabMenuKey = useRef("");
  const restored = useRef(false);
  const [readyForLaunch, setReadyForLaunch] = useState(false);
  const skipEmptySave = useRef(false);
  tabsRef.current = tabs;
  activeRef.current = active;

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
      setAgentAvailable(status.agentAvailable);
      setKeepalive(status.keepaliveSecs);
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
        setAgentAvailable(status.agentAvailable);
        setKeepalive(status.keepaliveSecs);
        await loadHosts();
      },
    });
  }

  useEffect(() => {
    if (!enabled || hosts.length === 0 || restored.current) return;
    restored.current = true;
    if (tabsRef.current.length > 0) {
      setReadyForLaunch(true);
      return;
    }
    const opened = readOpenHosts().flatMap((id) => {
      const host = hosts.find((item) => item.id === id);
      return host ? [host] : [];
    });
    if (opened.length === 0) {
      writeOpenHosts([]);
      setReadyForLaunch(true);
      return;
    }
    const next = opened.map((host) => ({
      key: `ssh-${host.id}-${(tabSerial += 1)}`,
      host,
      generation: 0,
      sessionId: null,
      phase: "connecting",
    }));
    skipEmptySave.current = true;
    setTabs(next);
    setActive(next[next.length - 1]?.key ?? "");
    setSelectedId(next[next.length - 1]?.host.id ?? null);
    setReadyForLaunch(true);
  }, [enabled, hosts]);

  useEffect(() => {
    if (!restored.current) return;
    if (tabs.length === 0 && skipEmptySave.current) {
      skipEmptySave.current = false;
      return;
    }
    writeOpenHosts(tabs.map((tab) => tab.host.id));
  }, [tabs]);

  const location = useLocation();
  const navigate = useNavigate();
  const launchedHost = useRef<number | null>(null);

  function connect(host: SshHost, fresh = false) {
    rememberHost(host.id);
    setError("");
    setSelectedId(host.id);
    const existing = fresh ? null : focusExistingSession(tabsRef.current.map((tab) => ({ key: tab.key, hostId: tab.host.id, phase: tab.phase })), host.id);
    if (existing) {
      setActive(existing.key);
      if (existing.reconnect) reconnectTab(existing.key);
      return;
    }
    const key = `ssh-${host.id}-${tabSerial += 1}`;
    setTabs((current) => current.some((tab) => tab.host.id === host.id) && !fresh ? current : [...current, { key, host, generation: 0, sessionId: null, phase: "connecting" }]);
    setActive(key);
  }

  useEffect(() => {
    if (!readyForLaunch) return;
    const hostId = (location.state as { hostId?: number } | null)?.hostId;
    if (!hostId) {
      launchedHost.current = null;
      return;
    }
    if (!enabled || hosts.length === 0 || launchedHost.current === hostId) return;
    const host = hosts.find((item) => item.id === hostId);
    if (!host) return;
    launchedHost.current = hostId;
    connect(host);
    navigate("/ssh", { replace: true, state: null });
  }, [enabled, hosts, location.state, navigate, readyForLaunch]);

  function applyClose(shouldClose: (tab: OpenTab, index: number) => boolean, preferKey?: string) {
    const result = tabsAfterClose(tabsRef.current, activeRef.current, shouldClose, preferKey);
    setTabs(result.tabs);
    setActive(result.activeKey);
  }

  function closeTab(key: string) {
    applyClose((tab) => tab.key === key);
  }

  function reconnectTab(key: string) {
    setTabs((current) => current.map((item) => item.key === key ? { ...item, generation: item.generation + 1, phase: "connecting", sessionId: null } : item));
  }

  function tabKeyFrom(target: EventTarget | null) {
    if (!(target instanceof Element)) return null;
    return target.closest(".ant-tabs-nav [data-node-key]")?.getAttribute("data-node-key") ?? null;
  }

  function runTabMenu(action: string) {
    const key = tabMenuKey.current;
    setTabMenu(null);
    if (!key) return;
    const tab = tabsRef.current.find((item) => item.key === key);
    if (!tab) return;
    const index = tabsRef.current.findIndex((item) => item.key === key);
    if (action === "duplicate") connect(tab.host, true);
    if (action === "copy") void navigator.clipboard.writeText(sshEndpoint(tab.host)).catch(() => undefined);
    if (action === "reconnect") reconnectTab(key);
    if (action === "disconnect" && tab.sessionId) {
      void sshDisconnect(tab.sessionId).catch((caught) => {
        if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
      });
    }
    if (action === "close") closeTab(key);
    if (action === "closeOthers") applyClose((item) => item.key !== key, key);
    if (action === "closeRight") applyClose((_item, itemIndex) => itemIndex > index, key);
    if (action === "closeAll") applyClose(() => true);
  }

  async function removeHost(host: SshHost, detach = false) {
    try {
      await sshDeleteHost(host.id, detach);
      setTabs((current) => current.filter((tab) => tab.host.id !== host.id));
      await loadHosts();
    } catch (caught) {
      if (caught instanceof SshRequestError && caught.code === "jump_in_use") {
        Modal.confirm({
          title: t("zdx.text.ssh.jumpInUseTitle", { name: host.name }),
          content: t("zdx.text.ssh.jumpInUseBody", { hosts: String(caught.params.hosts ?? "") }),
          okText: t("zdx.label.ssh.deleteHost"),
          cancelText: t("zdx.label.ssh.cancel"),
          okButtonProps: { danger: true },
          onOk: () => removeHost(host, true),
        });
        return;
      }
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
      const hostName = event.type === "prompt" && event.hostName ? event.hostName : tab.host.name;
      setDecision({ sessionId, hostName, event });
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
            onImport={() => setImportOpen(true)}
            keepalive={keepalive}
            onKeepalive={(seconds) => {
              setKeepalive(seconds);
              void sshSetKeepalive(seconds).then((status) => setKeepalive(status.keepaliveSecs)).catch((caught) => {
                if (caught instanceof SshRequestError) setError(sshErrorText(t, caught));
              });
            }}
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
              <div
                className="flex min-h-0 min-w-0 flex-1 flex-col"
                onContextMenu={(event) => {
                  const key = tabKeyFrom(event.target);
                  if (!key || !tabsRef.current.some((tab) => tab.key === key)) return;
                  event.preventDefault();
                  tabMenuKey.current = key;
                  setActive(key);
                  setTabMenu({ key, x: event.clientX, y: event.clientY });
                }}
                onMouseDown={(event) => {
                  if (event.button === 1 && tabKeyFrom(event.target)) event.preventDefault();
                }}
                onAuxClick={(event) => {
                  if (event.button !== 1) return;
                  const key = tabKeyFrom(event.target);
                  if (!key) return;
                  event.preventDefault();
                  closeTab(key);
                }}
              >
                <Tabs
                  className="min-h-0 flex-1 [&_.ant-tabs-nav]:px-3 [&_.ant-tabs-nav]:pt-2 [&_.ant-tabs-body-holder]:flex [&_.ant-tabs-body-holder]:min-h-0 [&_.ant-tabs-body-holder]:flex-1 [&_.ant-tabs-body-holder]:flex-col"
                  classNames={{ body: "min-h-0 flex-1", content: "h-full min-h-0 [&.ant-tabs-content-active]:flex [&.ant-tabs-content-active]:flex-col" }}
                  styles={{
                    root: { display: "flex", flexDirection: "column", flex: 1, minHeight: 0 },
                    body: { height: "100%", minHeight: 0, display: "flex", flexDirection: "column" },
                    content: { flex: 1, height: "100%", minHeight: 0 },
                  }}
                  type="editable-card"
                  hideAdd
                  activeKey={active}
                  onChange={setActive}
                  onEdit={(key, action) => {
                    if (action !== "remove" || typeof key !== "string") return;
                    closeTab(key);
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
                        onReconnect={() => reconnectTab(tab.key)}
                        onClose={() => closeTab(tab.key)}
                      />
                    ),
                  }))}
                />
                {tabMenu && (
                  <Dropdown
                    open
                    trigger={["click"]}
                    getPopupContainer={() => document.body}
                    onOpenChange={(open) => { if (!open) setTabMenu(null); }}
                    menu={{
                      items: [
                        { key: "duplicate", label: t("zdx.label.ssh.duplicateSession") },
                        { key: "copy", label: t("zdx.label.ssh.copyConnection") },
                        { type: "divider" },
                        { key: "reconnect", label: t("zdx.label.ssh.reconnect") },
                        { key: "disconnect", label: t("zdx.label.ssh.disconnect"), disabled: !tabs.some((tab) => tab.key === tabMenu.key && tab.sessionId && tab.phase !== "closed") },
                        { type: "divider" },
                        { key: "close", label: t("zdx.label.ssh.closeSession") },
                        { key: "closeOthers", label: t("zdx.label.ssh.closeOtherSessions"), disabled: tabs.length < 2 },
                        { key: "closeRight", label: t("zdx.label.ssh.closeSessionsToRight"), disabled: tabs.findIndex((tab) => tab.key === tabMenu.key) >= tabs.length - 1 },
                        { key: "closeAll", label: t("zdx.label.ssh.closeAllSessions") },
                      ],
                      onClick: ({ key }) => runTabMenu(key),
                    }}
                  >
                    <span className="fixed z-30 h-px w-px" style={{ left: tabMenu.x, top: tabMenu.y }} />
                  </Dropdown>
                )}
              </div>
            )}
          </>
        )}
      />
      <HostForm
        host={editing}
        hosts={hosts}
        open={formOpen}
        keyringAvailable={keyringAvailable}
        agentAvailable={agentAvailable}
        onClose={() => setFormOpen(false)}
        onSaved={() => void loadHosts()}
      />
      <ImportConfig open={importOpen} onClose={() => setImportOpen(false)} onImported={() => void loadHosts()} />
      <SessionPrompt
        decision={decision}
        onDone={() => setDecision(null)}
        onHostKey={(sessionId, accept) => sshHostKeyDecide(sessionId, accept)}
        onAnswer={(sessionId, promptId, secret, remember, cancel) => sshAnswer(sessionId, promptId, secret, remember, cancel)}
      />
    </div>
  );
}
