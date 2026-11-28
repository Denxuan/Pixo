import { DockerRequestError, dockerImageRun, dockerImages, dockerNetworks } from "@/api/docker";
import HostFolder from "@/components/docker/HostFolder";
import type { ContainerRunRequest, ImageRow, NetworkRow } from "@/types/docker";
import { dockerErrorText } from "@/utils/dockerMessage";

type MountRow = {
  id: number;
  kind: "bind" | "volume";
  source: string;
  target: string;
  readOnly: boolean;
};

type EnvRow = {
  id: number;
  key: string;
  value: string;
};

type PortRow = {
  id: number;
  host: string;
  container: string;
};

type NetMode = "custom" | "host" | "none";

const LAST_STEP = 4;

export default function RunDialog({
  endpointId,
  open,
  image,
  onClose,
  onDone,
  onError,
}: {
  endpointId: number;
  open: boolean;
  image: ImageRow | null;
  onClose: () => void;
  onDone: () => void;
  onError: (caught: unknown) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [imageRef, setImageRef] = useState("");
  const [catalog, setCatalog] = useState<ImageRow[]>([]);
  const [restart, setRestart] = useState("no");
  const [network, setNetwork] = useState("");
  const [portRows, setPortRows] = useState<PortRow[]>([]);
  const [mounts, setMounts] = useState<MountRow[]>([]);
  const [envRows, setEnvRows] = useState<EnvRow[]>([]);
  const [command, setCommand] = useState("");
  const [workdir, setWorkdir] = useState("");
  const [networks, setNetworks] = useState<NetworkRow[]>([]);
  const [folderFor, setFolderFor] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");
  const nextId = useRef(1);
  const seededNetwork = useRef(false);
  const mode: NetMode = network === "host" ? "host" : network === "none" ? "none" : "custom";
  const hostNetwork = mode !== "custom";
  const published = publishedPorts(portRows);
  const listed = networks.filter((row) => row.name && row.name !== "host" && row.name !== "none");
  const selectedNetwork = listed.find((row) => row.name === network) ?? null;

  useEffect(() => {
    if (!open) return;
    setName("");
    setImageRef(image ? image.tags[0] || image.id : "");
    setRestart("no");
    setNetwork("");
    setPortRows([]);
    setMounts([]);
    setEnvRows([]);
    setCommand("");
    setWorkdir("");
    setNetworks([]);
    setFolderFor(null);
    setRunning(false);
    setError("");
    seededNetwork.current = false;
  }, [open, image]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setCatalog([]);
    void dockerImages(endpointId).then((rows) => {
      if (alive) setCatalog(rows);
    }).catch(() => undefined);
    void dockerNetworks(endpointId).then((rows) => {
      if (!alive) return;
      setNetworks(rows);
      if (seededNetwork.current) return;
      seededNetwork.current = true;
      setNetwork((current) => current === "" && rows.some((row) => row.name === "bridge") ? "bridge" : current);
    }).catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [endpointId, open]);

  function blank(kind: "mount" | "env" | "port") {
    const id = nextId.current;
    nextId.current += 1;
    if (kind === "mount") setMounts((rows) => [...rows, { id, kind: "bind", source: "", target: "", readOnly: false }]);
    else if (kind === "env") setEnvRows((rows) => [...rows, { id, key: "", value: "" }]);
    else setPortRows((rows) => [...rows, { id, host: "", container: "" }]);
  }

  function chooseMode(next: NetMode) {
    if (next === "host") setNetwork("host");
    else if (next === "none") setNetwork("none");
    else setNetwork((current) => current === "host" || current === "none" ? (listed.some((row) => row.name === "bridge") ? "bridge" : "") : current);
  }

  function invalid() {
    setError(t("zdx.text.docker.error.invalid_input"));
  }

  async function start() {
    const ports = published.text;
    const draft = { name, command, network, ports, mounts, envRows, workdir };
    if (!validImageRef(imageRef) || running || (!hostNetwork && !published.ok) || !ready(LAST_STEP, draft)) {
      if (!running) invalid();
      return;
    }
    setRunning(true);
    setError("");
    const request: ContainerRunRequest = {
      image: imageRef.trim(),
      name: name.trim(),
      restart,
      network,
      ports: hostNetwork ? "" : ports,
      mounts: mounts.filter((row) => row.source.trim() || row.target.trim()).map((row) => ({
        kind: row.kind,
        source: row.source.trim(),
        target: row.target.trim(),
        readOnly: row.readOnly,
      })),
      env: envRows.filter((row) => row.key.trim() || row.value.trim()).map((row) => ({
        key: row.key.trim(),
        value: row.value,
      })),
      command: command.trim(),
      workdir: workdir.trim(),
    };
    try {
      await dockerImageRun(endpointId, request);
      onDone();
    } catch (caught) {
      if (caught instanceof DockerRequestError) {
        setError(dockerErrorText(t, caught));
        if (caught.code === "disconnected" || caught.code === "unreachable" || caught.code === "timeout") onError(caught);
      }
      setRunning(false);
    }
  }

  const folderRow = mounts.find((row) => row.id === folderFor) ?? null;

  return (
    <Modal
      title={imageRef ? t("zdx.text.docker.runTitle", { name: imageRef }) : t("zdx.text.docker.createTitle")}
      open={open}
      centered
      width={760}
      footer={(
        <div className="flex justify-end gap-2">
          <Button disabled={running} onClick={onClose}>{t("zdx.label.docker.cancel")}</Button>
          <Button type="primary" loading={running} onClick={() => void start()}>{t("zdx.label.docker.run")}</Button>
        </div>
      )}
      onCancel={() => { if (!running) onClose(); }}
      destroyOnHidden
      styles={{
        wrapper: { overflow: "hidden" },
        container: { display: "flex", flexDirection: "column", maxHeight: "calc(100vh - 48px)" },
        header: { flexShrink: 0 },
        body: { flex: "1 1 auto", minHeight: 0, overflowY: "auto", overscrollBehavior: "contain" },
        footer: { flexShrink: 0 },
      }}
    >
      <Flex vertical gap={12}>
        {error && <Alert type="error" showIcon message={error} />}
        <Fold title={t("zdx.label.docker.stepBasic")}>
          <Field label={t("zdx.label.docker.containerName")} required>
            <Input
              value={name}
              disabled={running}
              aria-label={t("zdx.label.docker.containerName")}
              placeholder={t("zdx.placeholder.docker.containerName")}
              onChange={(event) => setName(event.target.value)}
            />
          </Field>
          <Field label={t("zdx.label.docker.image")} required>
            <Select
              showSearch
              value={imageRef || undefined}
              disabled={running}
              aria-label={t("zdx.label.docker.image")}
              placeholder={t("zdx.placeholder.docker.chooseImage")}
              options={imageChoices(catalog)}
              optionFilterProp="label"
              onChange={setImageRef}
            />
          </Field>
          <Field label={t("zdx.label.docker.restartPolicy")}>
            <Select
              value={restart}
              disabled={running}
              aria-label={t("zdx.label.docker.restartPolicy")}
              options={[
                { value: "no", label: t("zdx.label.docker.restartNo") },
                { value: "always", label: t("zdx.label.docker.restartAlways") },
                { value: "unless-stopped", label: t("zdx.label.docker.restartUnlessStopped") },
                { value: "on-failure", label: t("zdx.label.docker.restartOnFailure") },
              ]}
              onChange={setRestart}
            />
            {restart === "on-failure" && <Typography.Text type="secondary">{t("zdx.text.docker.restartOnFailureHint")}</Typography.Text>}
          </Field>
          <Field label={t("zdx.label.docker.workdir")}>
            <Input
              value={workdir}
              disabled={running}
              aria-label={t("zdx.label.docker.workdir")}
              placeholder={t("zdx.placeholder.docker.workdir")}
              onChange={(event) => setWorkdir(event.target.value)}
            />
            <Typography.Text type="secondary">{t("zdx.text.docker.workdirHint")}</Typography.Text>
          </Field>
        </Fold>
        <Fold title={t("zdx.label.docker.portsSection")}>
          <Typography.Text type="secondary">{hostNetwork ? t("zdx.text.docker.hostNetworkHint") : t("zdx.text.docker.portsHelp")}</Typography.Text>
          {portRows.map((row) => (
            <div key={row.id} className="flex items-center gap-2">
              <Input
                className="min-w-0 flex-1"
                value={row.host}
                disabled={running || hostNetwork}
                aria-label={t("zdx.label.docker.localPort")}
                placeholder={t("zdx.placeholder.docker.localPort")}
                onChange={(event) => setPortRows((rows) => rows.map((item) => item.id === row.id ? { ...item, host: event.target.value } : item))}
              />
              <Input
                className="min-w-0 flex-1"
                value={row.container}
                disabled={running || hostNetwork}
                aria-label={t("zdx.label.docker.containerPort")}
                placeholder={t("zdx.placeholder.docker.containerPort")}
                onChange={(event) => setPortRows((rows) => rows.map((item) => item.id === row.id ? { ...item, container: event.target.value } : item))}
              />
              <Button
                type="text"
                disabled={running}
                aria-label={t("zdx.label.docker.removePort")}
                icon={<CloseOutlined />}
                onClick={() => setPortRows((rows) => rows.filter((item) => item.id !== row.id))}
              />
            </div>
          ))}
          <Button className="self-start" disabled={running || hostNetwork || portRows.length >= 20} icon={<PlusOutlined />} onClick={() => blank("port")}>
            {t("zdx.label.docker.addPort")}
          </Button>
        </Fold>
        <Fold title={t("zdx.label.docker.network")}>
          <div className="flex flex-col gap-2">
            <span>{t("zdx.label.docker.netMode")}</span>
            <Radio.Group value={mode} disabled={running} onChange={(event) => chooseMode(event.target.value as NetMode)}>
              <Radio value="custom">{t("zdx.label.docker.netCustom")}</Radio>
              <Radio value="host">{t("zdx.label.docker.netHost")}</Radio>
              <Radio value="none">{t("zdx.label.docker.netNone")}</Radio>
            </Radio.Group>
          </div>
          {mode === "custom" && (
            <div className="flex flex-col gap-2">
              <span>{t("zdx.label.docker.netPick")}</span>
              {listed.length === 0 ? (
                <Typography.Text type="secondary">{t("zdx.text.docker.emptyNetworks")}</Typography.Text>
              ) : (
                <div className="flex flex-wrap gap-x-5 gap-y-2">
                  {listed.map((row) => (
                    <Checkbox
                      key={row.id || row.name}
                      checked={network === row.name}
                      disabled={running}
                      onChange={(event) => setNetwork(event.target.checked ? row.name : "")}
                    >
                      <span className="inline-block max-w-36 truncate align-bottom" title={row.name}>{row.name}</span>
                    </Checkbox>
                  ))}
                </div>
              )}
            </div>
          )}
          {selectedNetwork && (
            <Table
              size="small"
              pagination={false}
              rowKey="name"
              dataSource={[selectedNetwork]}
              columns={[
                { title: t("zdx.label.docker.netName"), dataIndex: "name", ellipsis: true },
                { title: t("zdx.label.docker.subnet"), dataIndex: "subnet", ellipsis: true, render: (value: string) => value || t("zdx.text.docker.unset") },
                { title: t("zdx.label.docker.gateway"), dataIndex: "gateway", ellipsis: true, render: (value: string) => value || t("zdx.text.docker.unset") },
                { title: t("zdx.label.docker.ipv4"), width: 88, render: () => t("zdx.label.docker.addressAuto") },
                { title: t("zdx.label.docker.ipv6"), width: 88, render: () => t(selectedNetwork.ipv6 ? "zdx.label.docker.addressAuto" : "zdx.label.docker.ipv6Off") },
              ]}
            />
          )}
        </Fold>
        <Fold title={t("zdx.label.docker.storageSection")}>
          <Typography.Text type="secondary">{t("zdx.text.docker.storageHelp")}</Typography.Text>
          {mounts.map((row) => (
            <div key={row.id} className="flex items-center gap-2">
              <Space.Compact className="min-w-0 flex-1">
                <Input
                  className="min-w-0"
                  value={row.source}
                  disabled={running}
                  aria-label={t("zdx.label.docker.hostPath")}
                  placeholder={t("zdx.placeholder.docker.mountSource")}
                  onChange={(event) => setMounts((rows) => rows.map((item) => item.id === row.id ? { ...item, kind: "bind", source: event.target.value } : item))}
                />
                <Button
                  disabled={running}
                  aria-label={t("zdx.label.docker.chooseFolder")}
                  icon={<FolderOutlined />}
                  onClick={() => setFolderFor(row.id)}
                />
              </Space.Compact>
              <Input
                className="min-w-0 flex-1"
                value={row.target}
                disabled={running}
                aria-label={t("zdx.label.docker.mountPath")}
                placeholder={t("zdx.placeholder.docker.mountPath")}
                onChange={(event) => setMounts((rows) => rows.map((item) => item.id === row.id ? { ...item, target: event.target.value } : item))}
              />
              <Select
                className="shrink-0"
                value={row.readOnly ? "ro" : "rw"}
                disabled={running}
                style={{ width: 128 }}
                aria-label={t("zdx.label.docker.mountAccess")}
                options={[
                  { value: "rw", label: t("zdx.label.docker.readWrite") },
                  { value: "ro", label: t("zdx.label.docker.readOnly") },
                ]}
                onChange={(access: "rw" | "ro") => setMounts((rows) => rows.map((item) => item.id === row.id ? { ...item, readOnly: access === "ro" } : item))}
              />
              <Button
                type="text"
                className="shrink-0"
                disabled={running}
                aria-label={t("zdx.label.docker.removeMount")}
                icon={<CloseOutlined />}
                onClick={() => setMounts((rows) => rows.filter((item) => item.id !== row.id))}
              />
            </div>
          ))}
          <Button className="self-start" disabled={running || mounts.length >= 32} icon={<PlusOutlined />} onClick={() => blank("mount")}>
            {t("zdx.label.docker.addPath")}
          </Button>
        </Fold>
        <Fold title={t("zdx.label.docker.env")}>
          <Typography.Text type="secondary">{t("zdx.text.docker.envHint")}</Typography.Text>
          {envRows.map((row) => (
            <div key={row.id} className="flex items-center gap-2">
              <Input
                className="min-w-0 flex-[2]"
                value={row.key}
                disabled={running}
                aria-label={t("zdx.label.docker.envKey")}
                placeholder={t("zdx.placeholder.docker.envKey")}
                onChange={(event) => setEnvRows((rows) => rows.map((item) => item.id === row.id ? { ...item, key: event.target.value } : item))}
              />
              <Input
                className="min-w-0 flex-[3]"
                value={row.value}
                disabled={running}
                aria-label={t("zdx.label.docker.envValue")}
                placeholder={t("zdx.placeholder.docker.envValue")}
                onChange={(event) => setEnvRows((rows) => rows.map((item) => item.id === row.id ? { ...item, value: event.target.value } : item))}
              />
              <Button
                type="text"
                disabled={running}
                aria-label={t("zdx.label.docker.removeEnv")}
                icon={<CloseOutlined />}
                onClick={() => setEnvRows((rows) => rows.filter((item) => item.id !== row.id))}
              />
            </div>
          ))}
          <Button className="self-start" disabled={running || envRows.length >= 64} icon={<PlusOutlined />} onClick={() => blank("env")}>
            {t("zdx.label.docker.addEnv")}
          </Button>
        </Fold>
        <Fold title={t("zdx.label.docker.command")}>
          <Typography.Text type="secondary">{t("zdx.text.docker.commandHint")}</Typography.Text>
          <Input.TextArea
            value={command}
            disabled={running}
            aria-label={t("zdx.label.docker.command")}
            placeholder={t("zdx.placeholder.docker.command")}
            autoSize={{ minRows: 4, maxRows: 8 }}
            onChange={(event) => setCommand(event.target.value)}
          />
        </Fold>
      </Flex>
      <HostFolder
        open={folderRow !== null}
        endpointId={endpointId}
        initial={folderRow?.source ?? ""}
        onClose={() => setFolderFor(null)}
        onPick={(source) => {
          const id = folderFor;
          setFolderFor(null);
          if (id === null) return;
          setMounts((rows) => rows.map((item) => item.id === id ? { ...item, kind: "bind", source } : item));
        }}
      />
    </Modal>
  );
}

function Fold({ title, children }: { title: string; children: React.ReactNode }) {
  const { token } = antdTheme.useToken();
  const [open, setOpen] = useState(true);
  const panelId = useId();
  return (
    <section style={{ border: `1px solid ${token.colorBorder}`, borderRadius: token.borderRadiusLG, overflow: "hidden" }}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        className="flex w-full cursor-pointer items-center gap-2 border-0 px-4 py-3 text-left"
        style={{ background: token.colorBgLayout, color: token.colorText, fontWeight: 600 }}
        onClick={() => setOpen((value) => !value)}
      >
        <RightOutlined aria-hidden="true" style={{ fontSize: 12, transform: open ? "rotate(90deg)" : undefined }} />
        <span>{title}</span>
      </button>
      {open && (
        <div id={panelId} className="flex flex-col gap-3 px-4 py-4" style={{ background: token.colorBgElevated }}>
          {children}
        </div>
      )}
    </section>
  );
}

function Field({ label, required, children }: { label: string; required?: boolean; children: React.ReactNode }) {
  const { token } = antdTheme.useToken();
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs" style={{ color: token.colorTextSecondary }}>
        {label}
        {required && <span style={{ color: token.colorError }}> *</span>}
      </span>
      {children}
    </label>
  );
}

function imageChoices(rows: ImageRow[]) {
  const seen = new Set<string>();
  const options: { value: string; label: string }[] = [];
  for (const row of rows) {
    const names = row.tags.length > 0 ? row.tags : [row.id];
    for (const name of names) {
      if (!name || seen.has(name) || !validImageRef(name)) continue;
      seen.add(name);
      options.push({ value: name, label: name });
    }
  }
  options.sort((left, right) => left.label.localeCompare(right.label));
  return options;
}

function validImageRef(value: string) {
  const name = value.trim();
  return name.length >= 1
    && name.length <= 255
    && !name.includes("..")
    && !name.startsWith("/")
    && !name.endsWith("/")
    && !name.startsWith(":")
    && !name.startsWith("@")
    && /^[A-Za-z0-9._\-/:@]+$/.test(name);
}

function publishedPorts(rows: PortRow[]) {
  const used = rows.filter((row) => row.host.trim() || row.container.trim());
  const ok = used.length <= 20 && used.every((row) => validPort(row.host) && validPort(row.container));
  const hosts = used.map((row) => Number(row.host.trim()));
  const containers = used.map((row) => Number(row.container.trim()));
  const unique = new Set(hosts).size === hosts.length && new Set(containers).size === containers.length;
  const text = used.map((row) => `${Number(row.host.trim())}:${Number(row.container.trim())}`).join(" ");
  return { text, ok: ok && unique && validPorts(text) };
}

function validPort(value: string) {
  if (!/^\d{1,5}$/.test(value.trim())) return false;
  const port = Number(value.trim());
  return port >= 1 && port <= 65535;
}

function ready(step: number, draft: {
  name: string;
  command: string;
  network: string;
  ports: string;
  mounts: MountRow[];
  envRows: EnvRow[];
  workdir: string;
}): boolean {
  if (step === 0) return validName(draft.name) && validCommand(draft.command);
  if (step === 1) return validNetwork(draft.network) && (draft.network === "host" || draft.network === "none" || validPorts(draft.ports));
  if (step === 2) {
    const targets = draft.mounts.map((row) => row.target.trim()).filter(Boolean);
    return draft.mounts.every(validMount) && new Set(targets).size === targets.length;
  }
  if (step === 3) {
    const keys = draft.envRows.map((row) => row.key.trim()).filter(Boolean);
    return draft.envRows.every(validEnv) && validWorkdir(draft.workdir) && new Set(keys).size === keys.length;
  }
  return ready(0, draft) && ready(1, draft) && ready(2, draft) && ready(3, draft);
}

function validName(value: string) {
  const name = value.trim();
  return /^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/.test(name);
}

function validCommand(value: string) {
  const command = value.trim();
  return command.length <= 512 && ![...command].some((char) => char.charCodeAt(0) < 32);
}

function validNetwork(value: string) {
  return value === "" || /^[A-Za-z0-9_.-]{1,128}$/.test(value);
}

function validPorts(value: string) {
  const parts = value.trim().split(/[\s,]+/).filter(Boolean);
  if (parts.length > 20) return false;
  return parts.every((part) => {
    const match = /^(\d{1,5})(?::(\d{1,5}))?(?:\/(?:tcp|udp))?$/.exec(part);
    if (!match) return false;
    const host = Number(match[1]);
    const container = Number(match[2] ?? match[1]);
    return host >= 1 && host <= 65535 && container >= 1 && container <= 65535;
  });
}

function validMount(row: MountRow) {
  const source = row.source.trim();
  const target = row.target.trim();
  if (!source && !target) return true;
  if (!absolutePath(target)) return false;
  if (row.kind === "bind") return absolutePath(source);
  return /^[A-Za-z0-9_.-]{1,128}$/.test(source);
}

function validEnv(row: EnvRow) {
  const key = row.key.trim();
  if (!key && !row.value.trim()) return true;
  return /^[A-Za-z_][A-Za-z0-9_]{0,127}$/.test(key) && row.value.length <= 4096 && ![...row.value].some((char) => char.charCodeAt(0) < 32);
}

function validWorkdir(value: string) {
  const path = value.trim();
  return path === "" || absolutePath(path);
}

function absolutePath(value: string) {
  if (!value || value.length > 1024 || value.includes("\0") || value.includes("\n") || value.includes("\r")) return false;
  if (value === "/" || value === "\\") return false;
  if (value.startsWith("/")) return true;
  return /^[A-Za-z]:[\\/]/.test(value);
}
