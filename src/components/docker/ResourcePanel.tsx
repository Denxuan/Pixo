import { Channel } from "@tauri-apps/api/core";

import {
  DockerRequestError,
  dockerContainerAction,
  dockerContainerInspect,
  dockerComposeForget,
  dockerComposeImport,
  dockerComposeImportEngine,
  dockerComposePreview,
  dockerComposeProjects,
  dockerContainerLogs,
  dockerContainers,
  dockerImageInspect,
  dockerImagePull,
  dockerImageRemove,
  dockerImages,
  dockerPickFile,
  dockerNetworkInspect,
  dockerNetworkRemove,
  dockerNetworks,
  dockerTaskStop,
  dockerVolumeInspect,
  dockerVolumeRemove,
  dockerVolumes,
} from "@/api/docker";
import type { ComposePreview, ComposeProject, ContainerRow, DockerLogEvent, DockerPullEvent, EngineSummary, ImageRow, NetworkRow, VolumeRow } from "@/types/docker";
import { dockerErrorText, formatBytes, formatTime } from "@/utils/dockerMessage";

type TabKey = "containers" | "images" | "networks" | "volumes" | "compose";

export default function ResourcePanel({ endpointId, summary }: { endpointId: number; summary: EngineSummary }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<TabKey>("containers");
  const [tick, setTick] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [containers, setContainers] = useState<ContainerRow[]>([]);
  const [images, setImages] = useState<ImageRow[]>([]);
  const [networks, setNetworks] = useState<NetworkRow[]>([]);
  const [volumes, setVolumes] = useState<VolumeRow[]>([]);
  const [projects, setProjects] = useState<ComposeProject[]>([]);
  const [preview, setPreview] = useState<ComposePreview | null>(null);
  const [importing, setImporting] = useState(false);
  const [inspect, setInspect] = useState<{ title: string; body: string } | null>(null);
  const [logs, setLogs] = useState<{ id: string; name: string } | null>(null);
  const [pullOpen, setPullOpen] = useState(false);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError("");
    const load = tab === "containers"
      ? dockerContainers(endpointId).then((rows) => { if (alive) setContainers(rows); })
      : tab === "images"
        ? dockerImages(endpointId).then((rows) => { if (alive) setImages(rows); })
        : tab === "networks"
          ? dockerNetworks(endpointId).then((rows) => { if (alive) setNetworks(rows); })
          : tab === "volumes"
            ? dockerVolumes(endpointId).then((rows) => { if (alive) setVolumes(rows); })
            : dockerComposeProjects(endpointId).then((rows) => { if (alive) setProjects(rows); });
    void load.catch((caught: unknown) => {
      if (alive && caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    }).finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [endpointId, tab, tick, t]);

  async function showInspect(title: string, load: () => Promise<unknown>) {
    setError("");
    try {
      const value = await load();
      setInspect({ title, body: JSON.stringify(value, null, 2) });
    } catch (caught) {
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    }
  }

  function confirmRemove(name: string, action: (force: boolean) => Promise<void>, running = false) {
    Modal.confirm({
      title: t("zdx.text.docker.removeConfirm", { name }),
      okText: t("zdx.label.docker.remove"),
      cancelText: t("zdx.label.docker.cancel"),
      okButtonProps: { danger: true },
      onOk: () => {
        if (!running) return action(false).then(() => setTick((value) => value + 1));
        return new Promise<void>((resolve) => {
          Modal.confirm({
            title: t("zdx.text.docker.forceConfirm", { name }),
            okText: t("zdx.label.docker.forceRemove"),
            cancelText: t("zdx.label.docker.cancel"),
            okButtonProps: { danger: true },
            onOk: () => action(true).then(() => setTick((value) => value + 1)),
            onCancel: () => resolve(),
            afterClose: () => resolve(),
          });
        });
      },
    });
  }

  function fail(caught: unknown) {
    if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
  }

  function runContainer(id: string, action: "start" | "stop" | "restart") {
    void dockerContainerAction(endpointId, id, action).then(() => setTick((value) => value + 1)).catch(fail);
  }

  const needle = query.trim().toLowerCase();
  const containerRows = containers.filter((row) => matches(needle, row.name, row.image, row.id, row.status));
  const imageRows = images.filter((row) => matches(needle, row.id, row.tags.join(" ")));
  const networkRows = networks.filter((row) => matches(needle, row.name, row.driver, row.id));
  const volumeRows = volumes.filter((row) => matches(needle, row.name, row.driver, row.mountpoint));
  const projectRows = projects.filter((row) => matches(needle, row.name, row.configPath, row.services.map((service) => service.name).join(" ")));

  function forgetCompose(row: ComposeProject) {
    if (row.id === null) return;
    Modal.confirm({
      title: t("zdx.text.docker.forgetCompose", { name: row.name }),
      okText: t("zdx.label.docker.forgetCompose"),
      cancelText: t("zdx.label.docker.cancel"),
      onOk: () => dockerComposeForget(endpointId, row.id as number).then(() => setTick((value) => value + 1)).catch(fail),
    });
  }

  async function chooseCompose() {
    setError("");
    try {
      const path = await dockerPickFile(t("zdx.label.docker.importCompose"));
      if (!path) return;
      setPreview(await dockerComposePreview(path));
    } catch (caught) {
      fail(caught);
    }
  }

  async function importPreview() {
    if (!preview) return;
    setImporting(true);
    setError("");
    try {
      await dockerComposeImport(endpointId, preview.configPath);
      setPreview(null);
      setTick((value) => value + 1);
    } catch (caught) {
      fail(caught);
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Typography.Text>{t("zdx.text.docker.summary", {
          name: summary.name,
          version: summary.version,
          os: summary.os,
          arch: summary.arch,
          cpus: summary.cpus,
          memory: formatBytes(summary.memory),
          containers: summary.containers,
          images: summary.images,
        })}</Typography.Text>
        <Space>
          <Input allowClear value={query} placeholder={t("zdx.placeholder.docker.filter")} aria-label={t("zdx.placeholder.docker.filter")} onChange={(event) => setQuery(event.target.value)} />
          <ActionIcon label={t("zdx.label.docker.refresh")} icon={<ReloadOutlined />} onClick={() => setTick((value) => value + 1)} />
          {tab === "images" && <ActionIcon label={t("zdx.label.docker.pull")} icon={<CloudDownloadOutlined />} onClick={() => setPullOpen(true)} />}
          {tab === "compose" && <ActionIcon label={t("zdx.label.docker.importCompose")} icon={<FileAddOutlined />} onClick={() => void chooseCompose()} />}
        </Space>
      </div>
      {error && <Alert type="error" showIcon message={error} />}
      <Tabs
        activeKey={tab}
        onChange={(key) => setTab(key as TabKey)}
        items={[
          { key: "containers", label: t("zdx.label.docker.containers") },
          { key: "images", label: t("zdx.label.docker.images") },
          { key: "networks", label: t("zdx.label.docker.networks") },
          { key: "volumes", label: t("zdx.label.docker.volumes") },
          { key: "compose", label: t("zdx.label.docker.compose") },
        ]}
      />
      {tab === "containers" && (
        <Table
          size="small"
          rowKey="id"
          loading={loading}
          dataSource={containerRows}
          pagination={false}
          locale={{ emptyText: t("zdx.text.docker.emptyContainers") }}
          scroll={{ x: "max-content" }}
          columns={[
            { title: t("zdx.label.docker.name"), dataIndex: "name", ellipsis: true },
            { title: t("zdx.label.docker.image"), dataIndex: "image", ellipsis: true },
            {
              title: t("zdx.label.docker.status"),
              ellipsis: true,
              render: (_, row) => (
                <span className="inline-flex max-w-full items-center gap-2">
                  <Tag className="m-0" color={stateColor(row.state)}>{stateLabel(t, row.state)}</Tag>
                  <span className="truncate">{row.status}</span>
                </span>
              ),
            },
            { title: t("zdx.label.docker.ports"), render: (_, row) => row.ports.join(", "), ellipsis: true },
            actionsColumn<ContainerRow>(t, 176, (_, row) => {
              const active = row.state === "running" || row.state === "restarting";
              return (
                <>
                  {active
                    ? <ActionIcon label={t("zdx.label.docker.stop")} icon={<StopOutlined />} onClick={() => runContainer(row.id, "stop")} />
                    : <ActionIcon label={t("zdx.label.docker.start")} icon={<CaretRightOutlined />} onClick={() => runContainer(row.id, "start")} />}
                  <ActionIcon label={t("zdx.label.docker.restart")} icon={<RedoOutlined />} onClick={() => runContainer(row.id, "restart")} />
                  <ActionIcon label={t("zdx.label.docker.logs")} icon={<FileTextOutlined />} onClick={() => setLogs({ id: row.id, name: row.name })} />
                  <ActionIcon label={t("zdx.label.docker.inspect")} icon={<EyeOutlined />} onClick={() => void showInspect(row.name, () => dockerContainerInspect(endpointId, row.id))} />
                  <ActionIcon label={t("zdx.label.docker.remove")} danger icon={<DeleteOutlined />} onClick={() => confirmRemove(row.name, (force) => dockerContainerAction(endpointId, row.id, "remove", force).catch(fail), active)} />
                </>
              );
            }),
          ]}
        />
      )}
      {tab === "images" && (
        <Table
          size="small"
          rowKey="id"
          loading={loading}
          dataSource={imageRows}
          pagination={false}
          locale={{ emptyText: t("zdx.text.docker.emptyImages") }}
          scroll={{ x: "max-content" }}
          columns={[
            { title: t("zdx.label.docker.tags"), render: (_, row) => row.tags.length ? row.tags.join(", ") : row.id.slice(0, 12), ellipsis: true },
            { title: t("zdx.label.docker.size"), width: 110, render: (_, row) => formatBytes(row.size) },
            { title: t("zdx.label.docker.created"), width: 180, render: (_, row) => formatTime(row.created) },
            actionsColumn<ImageRow>(t, 80, (_, row) => {
              const name = row.tags[0] || row.id;
              return (
                <>
                  <ActionIcon label={t("zdx.label.docker.inspect")} icon={<EyeOutlined />} onClick={() => void showInspect(name, () => dockerImageInspect(endpointId, name))} />
                  <ActionIcon label={t("zdx.label.docker.remove")} danger icon={<DeleteOutlined />} onClick={() => confirmRemove(name, () => dockerImageRemove(endpointId, name).catch(fail))} />
                </>
              );
            }),
          ]}
        />
      )}
      {tab === "networks" && (
        <Table
          size="small"
          rowKey="id"
          loading={loading}
          dataSource={networkRows}
          pagination={false}
          locale={{ emptyText: t("zdx.text.docker.emptyNetworks") }}
          scroll={{ x: "max-content" }}
          columns={[
            { title: t("zdx.label.docker.name"), dataIndex: "name", ellipsis: true },
            { title: t("zdx.label.docker.driver"), dataIndex: "driver", width: 120 },
            { title: t("zdx.label.docker.scope"), dataIndex: "scope", width: 120 },
            actionsColumn<NetworkRow>(t, 80, (_, row) => (
              <>
                <ActionIcon label={t("zdx.label.docker.inspect")} icon={<EyeOutlined />} onClick={() => void showInspect(row.name, () => dockerNetworkInspect(endpointId, row.name))} />
                <ActionIcon label={t("zdx.label.docker.remove")} danger icon={<DeleteOutlined />} onClick={() => confirmRemove(row.name, () => dockerNetworkRemove(endpointId, row.name).catch(fail))} />
              </>
            )),
          ]}
        />
      )}
      {tab === "volumes" && (
        <Table
          size="small"
          rowKey="name"
          loading={loading}
          dataSource={volumeRows}
          pagination={false}
          locale={{ emptyText: t("zdx.text.docker.emptyVolumes") }}
          scroll={{ x: "max-content" }}
          columns={[
            { title: t("zdx.label.docker.name"), dataIndex: "name", ellipsis: true },
            { title: t("zdx.label.docker.driver"), dataIndex: "driver", width: 120 },
            { title: t("zdx.label.docker.mount"), dataIndex: "mountpoint", ellipsis: true },
            actionsColumn<VolumeRow>(t, 80, (_, row) => (
              <>
                <ActionIcon label={t("zdx.label.docker.inspect")} icon={<EyeOutlined />} onClick={() => void showInspect(row.name, () => dockerVolumeInspect(endpointId, row.name))} />
                <ActionIcon label={t("zdx.label.docker.remove")} danger icon={<DeleteOutlined />} onClick={() => confirmRemove(row.name, () => dockerVolumeRemove(endpointId, row.name).catch(fail))} />
              </>
            )),
          ]}
        />
      )}
      {tab === "compose" && (
        <Table
          size="small"
          rowKey={(row) => `${row.origin}-${row.id ?? row.name}-${row.configPath}`}
          loading={loading}
          dataSource={projectRows}
          pagination={false}
          locale={{ emptyText: t("zdx.text.docker.emptyCompose") }}
          scroll={{ x: "max-content" }}
          columns={[
            { title: t("zdx.label.docker.name"), dataIndex: "name", ellipsis: true },
            { title: t("zdx.label.docker.services"), ellipsis: true, render: (_, row) => row.services.map((service) => service.image ? `${service.name} (${service.image})` : service.name).join(", ") },
            { title: t("zdx.label.docker.running"), width: 90, render: (_, row) => `${row.running}/${row.total}` },
            { title: t("zdx.label.docker.status"), width: 150, render: (_, row) => composeOrigin(t, row.origin) },
            { title: t("zdx.label.docker.configFile"), dataIndex: "configPath", ellipsis: true },
            actionsColumn<ComposeProject>(t, 56, (_, row) => (
              <>
                {!row.imported && <ActionIcon label={t("zdx.label.docker.importCompose")} icon={<ImportOutlined />} onClick={() => void dockerComposeImportEngine(endpointId, row.name).then(() => setTick((value) => value + 1)).catch(fail)} />}
                {row.imported && row.id !== null && <ActionIcon label={t("zdx.label.docker.forgetCompose")} icon={<DeleteOutlined />} onClick={() => forgetCompose(row)} />}
              </>
            )),
          ]}
        />
      )}
      <Modal
        title={t("zdx.text.docker.composeImportTitle")}
        open={preview !== null}
        okText={t("zdx.label.docker.importCompose")}
        cancelText={t("zdx.label.docker.cancel")}
        confirmLoading={importing}
        onCancel={() => setPreview(null)}
        onOk={() => void importPreview()}
        destroyOnHidden
      >
        <div className="mb-3">
          <div className="font-medium">{preview?.name}</div>
          <Typography.Text type="secondary">{preview?.configPath}</Typography.Text>
        </div>
        <Table
          size="small"
          pagination={false}
          rowKey="name"
          dataSource={preview?.services ?? []}
          locale={{ emptyText: t("zdx.text.docker.emptyCompose") }}
          columns={[
            { title: t("zdx.label.docker.services"), dataIndex: "name" },
            { title: t("zdx.label.docker.image"), dataIndex: "image", ellipsis: true },
            { title: t("zdx.label.docker.ports"), render: (_, row) => row.ports.join(", ") },
          ]}
        />
      </Modal>
      <InspectDrawer value={inspect} onClose={() => setInspect(null)} />
      <LogsDrawer endpointId={endpointId} target={logs} onClose={() => setLogs(null)} />
      <PullDialog open={pullOpen} endpointId={endpointId} onClose={() => setPullOpen(false)} onDone={() => setTick((value) => value + 1)} />
    </div>
  );
}

function actionsColumn<T>(t: (key: string) => string, width: number, render: (value: unknown, row: T, index: number) => ReactNode) {
  return {
    title: t("zdx.label.docker.actions"),
    width,
    fixed: "right" as const,
    align: "right" as const,
    render,
  };
}

function ActionIcon({ label, icon, danger, onClick }: { label: string; icon: ReactNode; danger?: boolean; onClick: () => void }) {
  return (
    <Tooltip title={label}>
      <Button type="text" size="small" danger={danger} title={label} aria-label={label} icon={icon} onClick={onClick} />
    </Tooltip>
  );
}

function stateColor(state: string) {
  if (state === "running") return "success";
  if (state === "restarting" || state === "paused" || state === "removing") return "warning";
  if (state === "dead") return "error";
  return "default";
}

function stateLabel(t: (key: string) => string, state: string) {
  const key = `zdx.text.docker.containerState.${state}`;
  const label = t(key);
  return label === key ? state : label;
}

function composeOrigin(t: (key: string) => string, origin: string) {
  if (origin === "engine") return t("zdx.text.docker.composeEngine");
  if (origin === "both") return t("zdx.text.docker.composeBoth");
  return t("zdx.text.docker.composeFile");
}

function matches(needle: string, ...parts: string[]) {
  if (!needle) return true;
  return parts.join(" ").toLowerCase().includes(needle);
}

function InspectDrawer({ value, onClose }: { value: { title: string; body: string } | null; onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Drawer title={value?.title || t("zdx.label.docker.inspect")} open={value !== null} width={640} onClose={onClose} extra={
      <Button onClick={() => { if (value) void navigator.clipboard.writeText(value.body).catch(() => undefined); }}>{t("zdx.label.docker.copyJson")}</Button>
    }>
      <pre className="m-0 overflow-auto whitespace-pre-wrap break-all text-xs">{value?.body}</pre>
    </Drawer>
  );
}

function LogsDrawer({
  endpointId,
  target,
  onClose,
}: {
  endpointId: number;
  target: { id: string; name: string } | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const [lines, setLines] = useState<{ stream: string; text: string }[]>([]);
  const [error, setError] = useState("");
  const taskRef = useRef(0);

  useEffect(() => {
    if (!target) return;
    const events = new Channel<DockerLogEvent>();
    const tails = { stdout: "", stderr: "" };
    let task = 0;
    let stop = false;
    const push = (stream: string, text: string) => {
      setLines((current) => {
        const next = current.concat({ stream, text });
        return next.length > 2000 ? next.slice(next.length - 2000) : next;
      });
    };
    events.onmessage = (event) => {
      if (stop) return;
      if (event.type === "closed") {
        if (tails.stdout) push("stdout", tails.stdout);
        if (tails.stderr) push("stderr", tails.stderr);
        tails.stdout = "";
        tails.stderr = "";
        if (event.code !== "ok") setError(dockerErrorText(t, { code: event.code, params: event.params ?? {} }));
        return;
      }
      const key = event.stream === "stderr" ? "stderr" : "stdout";
      const parts = `${tails[key]}${event.text}`.split("\n");
      tails[key] = parts.pop() ?? "";
      for (const text of parts) push(event.stream, text);
    };
    void dockerContainerLogs(endpointId, target.id, events).then((id) => {
      if (stop) {
        void dockerTaskStop(id).catch(() => undefined);
        return;
      }
      task = id;
      taskRef.current = id;
    }).catch((caught: unknown) => {
      if (!stop && caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    });
    return () => {
      stop = true;
      const id = task || taskRef.current;
      if (id) void dockerTaskStop(id).catch(() => undefined);
      taskRef.current = 0;
      setLines([]);
      setError("");
    };
  }, [endpointId, target, t]);

  return (
    <Drawer title={target?.name || t("zdx.label.docker.logs")} open={target !== null} width={720} onClose={onClose}>
      {error && <Alert className="mb-3" type="error" showIcon message={error} />}
      {lines.length === 0 && !error && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.docker.logsEmpty")} />}
      <pre className="m-0 whitespace-pre-wrap break-all font-mono text-xs">
        {lines.map((line, index) => (
          <div key={index} style={{ color: line.stream === "stderr" ? token.colorError : token.colorText }}>{line.text}</div>
        ))}
      </pre>
    </Drawer>
  );
}

function PullDialog({
  open,
  endpointId,
  onClose,
  onDone,
}: {
  open: boolean;
  endpointId: number;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const [reference, setReference] = useState("");
  const [lines, setLines] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [running, setRunning] = useState(false);
  const taskRef = useRef(0);
  const stopped = useRef(false);

  useEffect(() => {
    if (open) return;
    setReference("");
    setLines([]);
    setError("");
    setRunning(false);
  }, [open]);

  function stop() {
    stopped.current = true;
    const id = taskRef.current;
    taskRef.current = 0;
    if (id) void dockerTaskStop(id).catch(() => undefined);
    setRunning(false);
  }

  function start() {
    const name = reference.trim();
    if (!name) return;
    Modal.confirm({
      title: t("zdx.text.docker.pullConfirm", { name }),
      okText: t("zdx.label.docker.pull"),
      cancelText: t("zdx.label.docker.cancel"),
      onOk: () => {
        const events = new Channel<DockerPullEvent>();
        events.onmessage = (event) => {
          if (event.type === "closed") {
            setRunning(false);
            taskRef.current = 0;
            if (event.code === "ok") {
              onDone();
              onClose();
            } else {
              setError(dockerErrorText(t, { code: event.code, params: event.params ?? {} }));
            }
            return;
          }
          const label = event.id ? `${event.id} ${event.status}` : event.status;
          setLines((current) => [...current.slice(-20), label]);
        };
        stopped.current = false;
        setRunning(true);
        setError("");
        setLines([]);
        void dockerImagePull(endpointId, name, events).then((id) => {
          if (stopped.current) {
            void dockerTaskStop(id).catch(() => undefined);
            return;
          }
          taskRef.current = id;
        }).catch((caught: unknown) => {
          if (stopped.current) return;
          setRunning(false);
          if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
        });
      },
    });
  }

  return (
    <Modal
      title={t("zdx.text.docker.pullTitle")}
      open={open}
      okText={running ? t("zdx.label.docker.cancel") : t("zdx.label.docker.pull")}
      cancelText={t("zdx.label.docker.closeLogs")}
      confirmLoading={false}
      onCancel={() => { stop(); onClose(); }}
      onOk={() => { if (running) stop(); else start(); }}
      destroyOnHidden
    >
      <Input value={reference} disabled={running} placeholder={t("zdx.placeholder.docker.image")} aria-label={t("zdx.label.docker.reference")} onChange={(event) => setReference(event.target.value)} />
      {error && <Alert className="mt-3" type="error" showIcon message={error} />}
      {lines.length > 0 && <pre className="mt-3 max-h-60 overflow-auto whitespace-pre-wrap text-xs">{lines.join("\n")}</pre>}
      {running && lines.length === 0 && <div className="mt-3 text-sm">{t("zdx.text.docker.pullProgress")}</div>}
    </Modal>
  );
}
