import { Channel } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

import {
  DockerRequestError,
  dockerContainerAction,
  dockerContainerInspect,
  dockerComposeAction,
  dockerComposeCreate,
  dockerComposeFile,
  dockerComposeProjects,
  dockerComposeRemove,
  dockerContainers,
  dockerContainerStats,
  dockerImageInspect,
  dockerImagePull,
  dockerImageRemove,
  dockerImageRun,
  dockerImages,
  dockerPickFile,
  dockerSummary,
  dockerNetworkInspect,
  dockerNetworkRemove,
  dockerNetworks,
  dockerVolumeInspect,
  dockerVolumeRemove,
  dockerVolumes,
  dockerTaskStop,
} from "@/api/docker";
import DockerShell from "@/components/docker/DockerShell";
import LogsDialog from "@/components/docker/LogsDialog";
import YamlEditor from "@/components/docker/YamlEditor";
import type { ContainerUsage } from "@/api/docker";
import type { ComposeProject, ContainerRow, DockerPullEvent, EngineSummary, ImageRow, NetworkRow, VolumeRow } from "@/types/docker";
import { dockerErrorText, formatBytes, formatTime } from "@/utils/dockerMessage";

type TabKey = "overview" | "containers" | "images" | "networks" | "volumes" | "compose";

type ComposeSource = "file" | "yaml";

export default function ResourcePanel({
  endpointId,
  summary,
  host,
  onLost,
}: {
  endpointId: number;
  summary: EngineSummary;
  host: string;
  onLost?: () => void;
}) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const [tab, setTab] = useState<TabKey>("overview");
  const [engine, setEngine] = useState(summary);
  const [tick, setTick] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [containers, setContainers] = useState<ContainerRow[]>([]);
  const [images, setImages] = useState<ImageRow[]>([]);
  const [networks, setNetworks] = useState<NetworkRow[]>([]);
  const [volumes, setVolumes] = useState<VolumeRow[]>([]);
  const [shell, setShell] = useState<{ id: string; name: string } | null>(null);
  const [projects, setProjects] = useState<ComposeProject[]>([]);
  const [createOpen, setCreateOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [directory, setDirectory] = useState("");
  const [source, setSource] = useState<ComposeSource>("file");
  const [composeFile, setComposeFile] = useState("");
  const [yaml, setYaml] = useState("");
  const [startProject, setStartProject] = useState(false);
  const [inspect, setInspect] = useState<{ title: string; body: string } | null>(null);
  const [composeView, setComposeView] = useState<{ name: string; body: string } | null>(null);
  const [logs, setLogs] = useState<{ name: string; containerId?: string } | null>(null);
  const [acting, setActing] = useState(false);
  const [pullOpen, setPullOpen] = useState(false);
  const [runImage, setRunImage] = useState<ImageRow | null>(null);
  const [notice, setNotice] = useState("");
  const [usage, setUsage] = useState<Record<string, ContainerUsage>>({});
  const lostRef = useRef(onLost);
  lostRef.current = onLost;

  useEffect(() => {
    setEngine(summary);
  }, [summary]);

  useEffect(() => {
    if (tab === "overview") return;
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
      if (alive) report(caught);
    }).finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [endpointId, tab, tick, t]);

  useEffect(() => {
    if (tab !== "containers") return;
    const running = containers.filter((row) => row.state === "running" || row.state === "restarting");
    if (running.length === 0) {
      setUsage({});
      return;
    }
    let alive = true;
    void (async () => {
      const next: Record<string, ContainerUsage> = {};
      for (let index = 0; index < running.length; index += 4) {
        const slice = running.slice(index, index + 4);
        await Promise.all(slice.map(async (row) => {
          try {
            next[row.id] = await dockerContainerStats(endpointId, row.id);
          } catch {
            return;
          }
        }));
        if (!alive) return;
        setUsage({ ...next });
      }
    })();
    return () => { alive = false; };
  }, [containers, endpointId, tab]);

  async function showInspect(title: string, load: () => Promise<unknown>) {
    setError("");
    try {
      const value = await load();
      setInspect({ title, body: JSON.stringify(value, null, 2) });
    } catch (caught) {
      report(caught);
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

  function report(caught: unknown) {
    if (!(caught instanceof DockerRequestError)) return;
    setError(dockerErrorText(t, caught));
    if (caught.code === "disconnected" || caught.code === "unreachable" || caught.code === "timeout") lostRef.current?.();
  }

  function fail(caught: unknown) {
    report(caught);
  }

  function runContainer(id: string, action: "start" | "stop" | "restart") {
    void dockerContainerAction(endpointId, id, action).then(() => setTick((value) => value + 1)).catch(fail);
  }

  function openSite(port: number) {
    if (!host) {
      setError(t("zdx.text.docker.openFailed"));
      return;
    }
    const scheme = port === 443 || port === 8443 ? "https" : "http";
    const target = host.includes(":") ? `[${host}]` : host;
    void openUrl(`${scheme}://${target}:${port}`).catch(() => setError(t("zdx.text.docker.openFailed")));
  }

  const needle = query.trim().toLowerCase();
  const containerRows = containers.filter((row) => matches(needle, row.name, row.image, row.id, row.status));
  const imageRows = images.filter((row) => matches(needle, row.id, row.tags.join(" ")));
  const networkRows = networks.filter((row) => matches(needle, row.name, row.driver, row.id));
  const volumeRows = volumes.filter((row) => matches(needle, row.name, row.driver, row.mountpoint));
  const projectRows = projects.filter((row) => matches(needle, row.name, row.configPath, row.services.map((service) => service.name).join(" ")));

  async function runCompose(name: string, action: "build" | "up" | "down") {
    setActing(true);
    setError("");
    try {
      await dockerComposeAction(endpointId, name, action);
      setTick((value) => value + 1);
    } catch (caught) {
      fail(caught);
    } finally {
      setActing(false);
    }
  }

  function confirmCompose(name: string, kind: "down" | "delete") {
    Modal.confirm({
      title: t(kind === "down" ? "zdx.text.docker.composeDownConfirm" : "zdx.text.docker.composeDeleteConfirm", { name }),
      okText: t(kind === "down" ? "zdx.label.docker.composeDown" : "zdx.label.docker.remove"),
      cancelText: t("zdx.label.docker.cancel"),
      okButtonProps: { danger: kind === "delete" },
      onOk: () => kind === "down" ? runCompose(name, "down") : dockerComposeRemove(endpointId, name).then(() => setTick((value) => value + 1)).catch(fail),
    });
  }

  async function showComposeFile(name: string) {
    setActing(true);
    setError("");
    try {
      setComposeView({ name, body: await dockerComposeFile(endpointId, name) });
    } catch (caught) {
      fail(caught);
    } finally {
      setActing(false);
    }
  }

  function openCreate() {
    setDraftName("");
    setDirectory("");
    setSource("file");
    setComposeFile("");
    setYaml("");
    setStartProject(false);
    setCreateOpen(true);
  }

  async function pickComposeFile() {
    try {
      const path = await dockerPickFile(t("zdx.label.docker.uploadCompose"));
      if (path) setComposeFile(path);
    } catch (caught) {
      fail(caught);
    }
  }

  async function createCompose() {
    if (!draftName.trim() || !directory.trim() || (source === "file" ? !composeFile : !yaml.trim())) {
      setError(t("zdx.text.docker.error.invalid_input"));
      return;
    }
    setCreating(true);
    setError("");
    try {
      await dockerComposeCreate(endpointId, {
        name: draftName.trim(),
        directory,
        source,
        filePath: source === "file" ? composeFile : "",
        yaml: source === "yaml" ? yaml : "",
        start: startProject,
      });
      setCreateOpen(false);
      setTick((value) => value + 1);
    } catch (caught) {
      fail(caught);
      setTick((value) => value + 1);
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div className="flex items-center justify-between gap-2">
        <Space>
          <ActionIcon label={t("zdx.label.docker.refresh")} icon={<ReloadOutlined />} onClick={() => {
            if (tab === "overview") {
              void dockerSummary(endpointId).then(setEngine).catch(fail);
              return;
            }
            setTick((value) => value + 1);
          }} />
          {tab === "images" && <ActionIcon label={t("zdx.label.docker.pull")} icon={<CloudDownloadOutlined />} onClick={() => setPullOpen(true)} />}
          {tab === "compose" && <ActionIcon label={t("zdx.label.docker.createCompose")} icon={<PlusOutlined />} onClick={openCreate} />}
        </Space>
        {tab !== "overview" && <Input allowClear style={{ width: 220 }} value={query} placeholder={t("zdx.placeholder.docker.filter")} aria-label={t("zdx.placeholder.docker.filter")} onChange={(event) => setQuery(event.target.value)} />}
      </div>
      {error && <Alert type="error" showIcon message={error} />}
      {notice && <Alert type="success" showIcon closable message={notice} onClose={() => setNotice("")} />}
      <Tabs
        activeKey={tab}
        onChange={(key) => setTab(key as TabKey)}
        items={[
          { key: "overview", label: t("zdx.label.docker.overview") },
          { key: "containers", label: t("zdx.label.docker.containers") },
          { key: "images", label: t("zdx.label.docker.images") },
          { key: "networks", label: t("zdx.label.docker.networks") },
          { key: "volumes", label: t("zdx.label.docker.volumes") },
          { key: "compose", label: t("zdx.label.docker.compose") },
        ]}
      />
      {tab === "overview" && <Overview summary={engine} />}
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
            {
              title: t("zdx.label.docker.image"),
              dataIndex: "image",
              width: 220,
              ellipsis: { showTitle: false },
              onCell: () => ({ style: { maxWidth: 220 } }),
              render: (value: string) => {
                if (!value) return null;
                return (
                  <Tooltip title={value} placement="topLeft" styles={{ container: { maxWidth: 480 } }}>
                    <span className="block truncate">{value}</span>
                  </Tooltip>
                );
              },
            },
            {
              title: t("zdx.label.docker.status"),
              ellipsis: true,
              render: (_, row) => {
                const current = usage[row.id];
                const usageLabel = current ? usageText(t, current) : "";
                return (
                  <span className="inline-flex max-w-full items-center gap-2">
                    <Tag className="m-0" color={stateColor(row.state)}>{stateLabel(t, row.state)}</Tag>
                    <span className="truncate">{row.status}</span>
                    {usageLabel && <span className="truncate text-xs" style={{ color: token.colorTextSecondary }}>{usageLabel}</span>}
                  </span>
                );
              },
            },
            {
              title: t("zdx.label.docker.ports"),
              width: 160,
              ellipsis: { showTitle: false },
              onCell: () => ({ style: { maxWidth: 160 } }),
              render: (_, row) => {
                const labels = [...new Set(row.ports)];
                if (labels.length === 0) return null;
                return (
                  <Tooltip
                    title={(
                      <span className="block">
                        {labels.map((label) => <span key={label} className="block whitespace-nowrap">{label}</span>)}
                      </span>
                    )}
                    placement="topLeft"
                    styles={{ container: { maxWidth: 280 } }}
                  >
                    <span className="block truncate">{labels.join(", ")}</span>
                  </Tooltip>
                );
              },
            },
            actionsColumn<ContainerRow>(t, 148, (_, row) => {
              const active = row.state === "running" || row.state === "restarting";
              const sites = publishedTcp(row.ports);
              return (
                <>
                  {sites.length > 0 && (
                    <Dropdown
                      trigger={["click"]}
                      placement="bottomRight"
                      menu={{
                        items: sites.map((site) => ({
                          key: String(site.hostPort),
                          icon: <LinkOutlined />,
                          label: `${site.hostPort}:${site.containerPort}`,
                        })),
                        onClick: ({ key }) => openSite(Number(key)),
                      }}
                    >
                      <Button type="text" size="small" title={t("zdx.label.docker.openSite")} aria-label={t("zdx.label.docker.openSite")} icon={<LinkOutlined />} />
                    </Dropdown>
                  )}
                  <ActionIcon
                    label={t(active ? "zdx.label.docker.stop" : "zdx.label.docker.start")}
                    icon={<PoweroffOutlined />}
                    onClick={() => runContainer(row.id, active ? "stop" : "start")}
                  />
                  <Dropdown
                    trigger={["click"]}
                    placement="bottomRight"
                    menu={{
                      items: [
                        { key: "restart", icon: <RedoOutlined />, label: t("zdx.label.docker.restart") },
                        { key: "details", icon: <InfoCircleOutlined />, label: t("zdx.label.docker.details") },
                        { key: "shell", icon: <CodeOutlined />, label: t("zdx.label.docker.shell"), disabled: !active },
                        { key: "logs", icon: <FileTextOutlined />, label: t("zdx.label.docker.runLogs") },
                        { type: "divider" },
                        { key: "remove", icon: <DeleteOutlined />, label: t("zdx.label.docker.remove"), danger: true },
                      ],
                      onClick: ({ key }) => {
                        if (key === "restart") runContainer(row.id, "restart");
                        if (key === "details") void showInspect(row.name, () => dockerContainerInspect(endpointId, row.id));
                        if (key === "shell") setShell({ id: row.id, name: row.name });
                        if (key === "logs") setLogs({ name: row.name, containerId: row.id });
                        if (key === "remove") confirmRemove(row.name, (force) => dockerContainerAction(endpointId, row.id, "remove", force).catch(fail), active);
                      },
                    }}
                  >
                    <Button type="text" size="small" title={t("zdx.label.docker.more")} aria-label={t("zdx.label.docker.more")} icon={<EllipsisOutlined />} />
                  </Dropdown>
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
            actionsColumn<ImageRow>(t, 148, (_, row) => {
              const name = row.tags[0] || row.id;
              return (
                <>
                  <ActionIcon label={t("zdx.label.docker.run")} icon={<PlayCircleOutlined />} onClick={() => { setNotice(""); setRunImage(row); }} />
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
            actionsColumn<NetworkRow>(t, 136, (_, row) => (
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
            { title: t("zdx.label.docker.scope"), dataIndex: "scope", width: 120 },
            actionsColumn<VolumeRow>(t, 136, (_, row) => (
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
          loading={loading || acting}
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
              <Dropdown
                trigger={["click"]}
                placement="bottomRight"
                menu={{
                  items: [
                    { key: "build", icon: <CodeOutlined />, label: t("zdx.label.docker.composeBuild") },
                    { key: "up", icon: <PlayCircleOutlined />, label: t("zdx.label.docker.composeUp") },
                    { key: "down", icon: <ClearOutlined />, label: t("zdx.label.docker.composeDown") },
                    { key: "delete", icon: <DeleteOutlined />, label: t("zdx.label.docker.remove"), danger: true },
                    { key: "details", icon: <InfoCircleOutlined />, label: t("zdx.label.docker.details") },
                    { key: "logs", icon: <FileTextOutlined />, label: t("zdx.label.docker.showLogs") },
                  ],
                  onClick: ({ key }) => {
                    if (key === "build" || key === "up") void runCompose(row.name, key);
                    if (key === "down" || key === "delete") confirmCompose(row.name, key);
                    if (key === "details") void showComposeFile(row.name);
                    if (key === "logs") setLogs({ name: row.name });
                  },
                }}
              >
                <Button type="text" size="small" title={t("zdx.label.docker.more")} aria-label={t("zdx.label.docker.more")} icon={<EllipsisOutlined />} />
              </Dropdown>
            )),
          ]}
        />
      )}
      <Modal
        title={t("zdx.text.docker.composeCreateTitle")}
        open={createOpen}
        centered
        width={source === "yaml" ? 720 : 560}
        okText={t("zdx.label.docker.confirm")}
        cancelText={t("zdx.label.docker.cancel")}
        confirmLoading={creating}
        onCancel={() => setCreateOpen(false)}
        onOk={() => void createCompose()}
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
      >
        <div className="flex flex-col gap-4">
          <label className="flex flex-col gap-2">
            <span>{t("zdx.label.docker.projectName")} <span style={{ color: token.colorError }}>*</span></span>
            <Input
              aria-label={t("zdx.label.docker.projectName")}
              placeholder={t("zdx.placeholder.docker.composeName")}
              value={draftName}
              onChange={(event) => setDraftName(event.target.value)}
            />
          </label>
          <label className="flex flex-col gap-2">
            <span>{t("zdx.label.docker.composePath")} <span style={{ color: token.colorError }}>*</span></span>
            <Input
              aria-label={t("zdx.label.docker.composePath")}
              placeholder={t("zdx.placeholder.docker.composeDirectory")}
              value={directory}
              spellCheck={false}
              className="font-mono"
              onChange={(event) => setDirectory(event.target.value)}
            />
            <Typography.Text type="secondary">{t("zdx.text.docker.composeDirectoryHint")}</Typography.Text>
          </label>
          <div className="flex flex-col gap-2">
            <span>{t("zdx.label.docker.composeSource")}</span>
            <Radio.Group value={source} onChange={(event) => setSource(event.target.value as ComposeSource)}>
              <Radio value="file">{t("zdx.label.docker.uploadCompose")}</Radio>
              <Radio value="yaml">{t("zdx.label.docker.writeCompose")}</Radio>
            </Radio.Group>
            {source === "file" ? (
              <Input
                readOnly
                aria-label={t("zdx.label.docker.uploadCompose")}
                placeholder={t("zdx.placeholder.docker.choose")}
                value={composeFile}
                onClick={() => void pickComposeFile()}
                suffix={<Button type="text" size="small" aria-label={t("zdx.label.docker.chooseComposeFile")} icon={<RightOutlined />} onClick={(event) => { event.stopPropagation(); void pickComposeFile(); }} />}
              />
            ) : (
              <YamlEditor
                label={t("zdx.label.docker.writeCompose")}
                placeholder={t("zdx.placeholder.docker.composeYaml")}
                value={yaml}
                onChange={setYaml}
              />
            )}
          </div>
          <Checkbox checked={startProject} onChange={(event) => setStartProject(event.target.checked)}>{t("zdx.label.docker.startCompose")}</Checkbox>
        </div>
      </Modal>
      <InspectDrawer value={inspect} onClose={() => setInspect(null)} />
      <LogsDialog endpointId={endpointId} target={logs} onClose={() => setLogs(null)} />
      <Drawer title={composeView?.name || t("zdx.label.docker.details")} open={composeView !== null} width={720} onClose={() => setComposeView(null)}>
        <pre className="m-0 overflow-auto whitespace-pre-wrap break-all font-mono text-xs">{composeView?.body}</pre>
      </Drawer>
      <PullDialog open={pullOpen} endpointId={endpointId} onClose={() => setPullOpen(false)} onDone={() => setTick((value) => value + 1)} />
      <DockerShell key={shell?.id ?? "shell"} endpointId={endpointId} container={shell} onClose={() => setShell(null)} />
      <RunDialog
        endpointId={endpointId}
        image={runImage}
        onClose={() => setRunImage(null)}
        onDone={() => {
          setRunImage(null);
          setNotice(t("zdx.text.docker.ran"));
          setTab("containers");
          setTick((value) => value + 1);
        }}
        onError={report}
      />
    </div>
  );
}

function publishedTcp(ports: string[]) {
  const seen = new Set<number>();
  const sites: { hostPort: number; containerPort: number }[] = [];
  for (const label of ports) {
    const match = /^(\d+)->(\d+)\/tcp$/.exec(label);
    if (!match) continue;
    const hostPort = Number(match[1]);
    const containerPort = Number(match[2]);
    if (!Number.isInteger(hostPort) || hostPort < 1 || hostPort > 65535 || seen.has(hostPort)) continue;
    seen.add(hostPort);
    sites.push({ hostPort, containerPort });
  }
  return sites;
}

function Overview({ summary }: { summary: EngineSummary }) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const items = [
    [t("zdx.label.docker.engine"), summary.name],
    [t("zdx.label.docker.version"), summary.version],
    [t("zdx.label.docker.hostname"), summary.hostname],
    [t("zdx.label.docker.os"), summary.os],
    [t("zdx.label.docker.arch"), summary.arch],
    [t("zdx.label.docker.cpus"), String(summary.cpus)],
    [t("zdx.label.docker.memory"), formatBytes(summary.memory)],
    [t("zdx.label.docker.containers"), String(summary.containers)],
    [t("zdx.label.docker.images"), String(summary.images)],
    ...(summary.socket ? [[t("zdx.label.docker.socket"), summary.socket]] : []),
  ];
  return (
    <dl className="m-0 grid grid-cols-2 gap-3">
      {items.map(([label, value]) => (
        <div key={label} className="rounded-md px-3 py-2" style={{ background: token.colorFillQuaternary }}>
          <dt className="text-xs" style={{ color: token.colorTextSecondary }}>{label}</dt>
          <dd className="m-0 mt-1 break-all" style={{ color: token.colorText }}>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function actionsColumn<T>(t: (key: string) => string, width: number, render: (value: unknown, row: T, index: number) => ReactNode) {
  const cell = { whiteSpace: "nowrap" as const, minWidth: width };
  return {
    title: t("zdx.label.docker.actions"),
    width,
    minWidth: width,
    fixed: "right" as const,
    align: "right" as const,
    onCell: () => ({ style: cell }),
    onHeaderCell: () => ({ style: cell }),
    render: (value: unknown, row: T, index: number) => (
      <div style={{ display: "inline-flex", flexFlow: "row nowrap", alignItems: "center", justifyContent: "flex-end" }}>{render(value, row, index)}</div>
    ),
  };
}

function ActionIcon({ label, icon, danger, onClick }: { label: string; icon: ReactNode; danger?: boolean; onClick: () => void }) {
  return (
    <Tooltip title={label}>
      <Button type="text" size="small" danger={danger} title={label} aria-label={label} icon={icon} style={{ flex: "none" }} onClick={onClick} />
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

function usageText(t: (key: string, options?: Record<string, unknown>) => string, usage: ContainerUsage) {
  if (usage.cpuPercent == null && usage.memoryBytes <= 0 && usage.memoryLimit == null) return "";
  const memory = formatBytes(usage.memoryBytes);
  const limit = usage.memoryLimit == null ? "" : formatBytes(usage.memoryLimit);
  const cpu = usage.cpuPercent == null ? "" : usage.cpuPercent.toFixed(1);
  if (cpu && limit) return t("zdx.text.docker.usageCpuMemoryLimit", { cpu, memory, limit });
  if (cpu) return t("zdx.text.docker.usageCpuMemory", { cpu, memory });
  if (limit) return t("zdx.text.docker.usageMemoryLimit", { memory, limit });
  return t("zdx.text.docker.usageMemory", { memory });
}

function RunDialog({
  endpointId,
  image,
  onClose,
  onDone,
  onError,
}: {
  endpointId: number;
  image: ImageRow | null;
  onClose: () => void;
  onDone: () => void;
  onError: (caught: unknown) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [ports, setPorts] = useState("");
  const [command, setCommand] = useState("");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const imageName = image ? image.tags[0] || image.id : "";

  useEffect(() => {
    if (image) return;
    setName("");
    setPorts("");
    setCommand("");
    setRunning(false);
    setError("");
  }, [image]);

  async function start() {
    if (!imageName || running) return;
    setRunning(true);
    setError("");
    try {
      await dockerImageRun(endpointId, imageName, name.trim(), ports.trim(), command.trim());
      onDone();
    } catch (caught) {
      if (caught instanceof DockerRequestError) {
        setError(dockerErrorText(t, caught));
        if (caught.code === "disconnected" || caught.code === "unreachable" || caught.code === "timeout") onError(caught);
      }
      setRunning(false);
    }
  }

  return (
    <Modal
      title={t("zdx.text.docker.runTitle", { name: imageName })}
      open={image !== null}
      okText={t("zdx.label.docker.run")}
      cancelText={t("zdx.label.docker.cancel")}
      confirmLoading={running}
      onCancel={onClose}
      onOk={() => void start()}
      destroyOnHidden
    >
      <Flex vertical gap={8}>
        {error && <Alert type="error" showIcon message={error} />}
        <Input value={name} disabled={running} aria-label={t("zdx.label.docker.containerName")} placeholder={t("zdx.placeholder.docker.containerName")} onChange={(event) => setName(event.target.value)} />
        <Input value={ports} disabled={running} aria-label={t("zdx.label.docker.publish")} placeholder={t("zdx.placeholder.docker.publish")} onChange={(event) => setPorts(event.target.value)} />
        <Typography.Text type="secondary">{t("zdx.text.docker.publishHint")}</Typography.Text>
        <Input value={command} disabled={running} aria-label={t("zdx.label.docker.command")} placeholder={t("zdx.placeholder.docker.command")} onChange={(event) => setCommand(event.target.value)} />
        <Typography.Text type="secondary">{t("zdx.text.docker.commandHint")}</Typography.Text>
      </Flex>
    </Modal>
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
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [needsAuth, setNeedsAuth] = useState(false);
  const [lines, setLines] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [running, setRunning] = useState(false);
  const taskRef = useRef(0);
  const stopped = useRef(false);

  useEffect(() => {
    if (open) return;
    setReference("");
    setUsername("");
    setPassword("");
    setRemember(true);
    setNeedsAuth(false);
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
              setNeedsAuth(event.code === "registry_auth");
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
        const auth = needsAuth && username.trim() && password ? { username: username.trim(), password, remember } : undefined;
        void dockerImagePull(endpointId, name, events, auth).then((id) => {
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
      okText={t("zdx.label.docker.pull")}
      cancelText={t("zdx.label.docker.cancel")}
      confirmLoading={running}
      onCancel={() => { stop(); onClose(); }}
      onOk={() => { if (!running) start(); }}
      destroyOnHidden
    >
      <Input value={reference} disabled={running} placeholder={t("zdx.placeholder.docker.image")} aria-label={t("zdx.label.docker.reference")} onChange={(event) => setReference(event.target.value)} />
      {needsAuth && (
        <Flex vertical gap={8} className="mt-3">
          <Typography.Text type="secondary">{t("zdx.text.docker.registryHint")}</Typography.Text>
          <Input value={username} disabled={running} autoComplete="off" aria-label={t("zdx.label.docker.registryUser")} placeholder={t("zdx.label.docker.registryUser")} onChange={(event) => setUsername(event.target.value)} />
          <Input.Password value={password} disabled={running} autoComplete="off" aria-label={t("zdx.label.docker.registryPassword")} placeholder={t("zdx.label.docker.registryPassword")} onChange={(event) => setPassword(event.target.value)} />
          <Checkbox checked={remember} disabled={running} onChange={(event) => setRemember(event.target.checked)}>{t("zdx.label.docker.rememberRegistry")}</Checkbox>
        </Flex>
      )}
      {error && <Alert className="mt-3" type="error" showIcon message={error} />}
      {lines.length > 0 && <pre className="mt-3 max-h-60 overflow-auto whitespace-pre-wrap text-xs">{lines.join("\n")}</pre>}
      {running && lines.length === 0 && <div className="mt-3 text-sm">{t("zdx.text.docker.pullProgress")}</div>}
    </Modal>
  );
}
