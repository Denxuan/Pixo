import type { DockerEndpoint, DockerKind } from "@/types/docker";

export default function EndpointList({
  endpoints,
  selectedId,
  sshEnabled,
  phase,
  onSelect,
  onCreate,
  onEdit,
  onDelete,
  onImport,
}: {
  endpoints: DockerEndpoint[];
  selectedId: number | null;
  sshEnabled: boolean;
  phase: string;
  onSelect: (endpoint: DockerEndpoint) => void;
  onCreate: () => void;
  onEdit: (endpoint: DockerEndpoint) => void;
  onDelete: (endpoint: DockerEndpoint) => void;
  onImport: () => void;
}) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const visible = endpoints.filter((endpoint) => {
    if (!needle) return true;
    const haystack = [endpoint.name, endpoint.host, endpoint.socketPath, endpoint.remoteSocket].filter(Boolean).join(" ").toLowerCase();
    return haystack.includes(needle);
  });

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 px-3 pb-2 pt-3">
        <Input
          allowClear
          value={query}
          placeholder={t("zdx.placeholder.docker.search")}
          aria-label={t("zdx.label.docker.search")}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      <div className="flex shrink-0 gap-2 px-3 pb-2">
        <Button type="primary" icon={<PlusOutlined />} onClick={onCreate}>{t("zdx.label.docker.newEndpoint")}</Button>
        <Button icon={<ImportOutlined />} aria-label={t("zdx.label.docker.importContexts")} onClick={onImport} />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {endpoints.length === 0 && (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.docker.emptyEndpoints")} />
        )}
        {endpoints.length > 0 && visible.length === 0 && (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.docker.noMatch")} />
        )}
        <div className="flex flex-col gap-1">
          {visible.map((endpoint) => {
            const selected = endpoint.id === selectedId;
            const sshOff = endpoint.kind === "ssh" && !sshEnabled;
            return (
              <div
                key={endpoint.id}
                className="rounded-lg px-2 py-2"
                style={{ background: selected ? token.colorFillSecondary : undefined }}
              >
                <button
                  type="button"
                  className="w-full cursor-pointer border-0 bg-transparent p-0 text-left"
                  onClick={() => onSelect(endpoint)}
                >
                  <div className="truncate text-sm font-medium" style={{ color: token.colorText }}>{endpoint.name}</div>
                  <div className="mt-1 flex flex-wrap items-center gap-1">
                    <Tag>{kindLabel(t, endpoint.kind)}</Tag>
                    {endpoint.insecure && <Tag color="warning">{t("zdx.text.docker.insecureBadge")}</Tag>}
                    {selected && phase === "ready" && <Tag color="success">{t("zdx.text.docker.ready")}</Tag>}
                    {selected && phase === "connecting" && <Tag>{t("zdx.text.docker.connecting")}</Tag>}
                    {selected && phase === "closed" && <Tag color="error">{t("zdx.text.docker.unreachableTitle")}</Tag>}
                  </div>
                  <div className="mt-1 truncate text-xs" style={{ color: token.colorTextTertiary }}>{endpointDetail(endpoint)}</div>
                  {sshOff && <div className="mt-1 text-xs" style={{ color: token.colorWarning }}>{t("zdx.text.docker.sshOff")}</div>}
                </button>
                <div className="mt-1 flex justify-end">
                  <Tooltip title={t("zdx.label.docker.editEndpoint")}>
                    <Button type="text" size="small" aria-label={t("zdx.label.docker.editEndpoint")} icon={<EditOutlined />} onClick={() => onEdit(endpoint)} />
                  </Tooltip>
                  <Tooltip title={t("zdx.label.docker.deleteEndpoint")}>
                    <Button type="text" size="small" danger aria-label={t("zdx.label.docker.deleteEndpoint")} icon={<DeleteOutlined />} onClick={() => onDelete(endpoint)} />
                  </Tooltip>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function kindLabel(t: (key: string) => string, kind: DockerKind) {
  if (kind === "local") return t("zdx.label.docker.kindLocal");
  if (kind === "ssh") return t("zdx.label.docker.kindSsh");
  if (kind === "tls") return t("zdx.label.docker.kindTls");
  return t("zdx.label.docker.kindTcp");
}

function endpointDetail(endpoint: DockerEndpoint) {
  if (endpoint.kind === "local") return endpoint.socketPath || "";
  if (endpoint.kind === "ssh") return endpoint.remoteSocket || "";
  if (endpoint.host) return `${endpoint.host}:${endpoint.port ?? ""}`;
  return "";
}
