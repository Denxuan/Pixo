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
  onSettings,
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
  onSettings: () => void;
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
        <Button icon={<SettingOutlined />} aria-label={t("zdx.label.nav.settings")} onClick={onSettings} />
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
            const detail = endpointDetail(endpoint);
            return (
              <div
                key={endpoint.id}
                className="flex items-start gap-1 rounded-lg px-2 py-1.5"
                style={{ background: selected ? token.colorFillSecondary : undefined }}
              >
                <button
                  type="button"
                  className="min-w-0 flex-1 cursor-pointer border-0 bg-transparent p-0 text-left"
                  onClick={() => onSelect(endpoint)}
                >
                  <div className="flex min-w-0 flex-wrap items-center gap-1">
                    <span className="truncate text-sm font-medium" style={{ color: token.colorText }}>{endpoint.name}</span>
                    <Tag style={badgeStyle}>{kindLabel(t, endpoint.kind)}</Tag>
                    {selected && phase === "ready" && <Tag color="success" style={badgeStyle}>{t("zdx.text.docker.ready")}</Tag>}
                    {selected && phase === "connecting" && <Tag style={badgeStyle}>{t("zdx.text.docker.connecting")}</Tag>}
                    {selected && phase === "closed" && <Tag color="error" style={badgeStyle}>{t("zdx.text.docker.unreachableTitle")}</Tag>}
                  </div>
                  {detail && <div className="truncate text-xs" style={{ color: token.colorTextTertiary }}>{detail}</div>}
                  {sshOff && <div className="text-xs" style={{ color: token.colorWarning }}>{t("zdx.text.docker.sshOff")}</div>}
                </button>
                <div className="flex shrink-0">
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

const badgeStyle = { margin: 0, fontSize: 12, lineHeight: "16px", paddingInline: 6 };

function kindLabel(t: (key: string) => string, kind: DockerKind) {
  if (kind === "local") return t("zdx.label.docker.kindLocal");
  return t("zdx.label.docker.kindSsh");
}

function endpointDetail(endpoint: DockerEndpoint) {
  if (endpoint.kind === "local") return endpoint.socketPath || "";
  return endpoint.remoteSocket || "";
}
