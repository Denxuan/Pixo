import type { SshHost } from "@/types/ssh";

export default function HostList({
  hosts,
  query,
  selectedId,
  onQuery,
  onSelect,
  onConnect,
  onEdit,
  onDelete,
  onCreate,
}: {
  hosts: SshHost[];
  query: string;
  selectedId: number | null;
  onQuery: (query: string) => void;
  onSelect: (id: number) => void;
  onConnect: (host: SshHost) => void;
  onEdit: (host: SshHost) => void;
  onDelete: (host: SshHost) => void;
  onCreate: () => void;
}) {
  const { t } = useTranslation();
  const needle = query.trim().toLocaleLowerCase();
  const visible = hosts.filter((host) => {
    if (!needle) return true;
    return [host.name, host.groupName ?? "", host.host, host.username].some((value) => value.toLocaleLowerCase().includes(needle));
  });
  const groups = new Map<string, SshHost[]>();
  for (const host of visible) {
    const key = host.groupName ?? "";
    const list = groups.get(key) ?? [];
    list.push(host);
    groups.set(key, list);
  }
  const sections = [...groups.entries()].sort((left, right) => left[0].localeCompare(right[0]));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 px-3 pb-3 pt-4">
        <Input
          className="min-w-0 flex-1"
          allowClear
          aria-label={t("zdx.label.ssh.search")}
          placeholder={t("zdx.placeholder.ssh.search")}
          prefix={<SearchOutlined />}
          value={query}
          onChange={(event) => onQuery(event.target.value)}
        />
        <Tooltip title={t("zdx.label.ssh.newHost")}>
          <Button aria-label={t("zdx.label.ssh.newHost")} icon={<PlusOutlined />} onClick={onCreate} />
        </Tooltip>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-2 pb-3" role="listbox" aria-label={t("zdx.label.ssh.hosts")}>
        {visible.length === 0 && (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={hosts.length === 0 ? t("zdx.text.ssh.emptyHosts") : t("zdx.text.ssh.noMatch")} />
        )}
        {sections.map(([group, items]) => (
          <section key={group || "ungrouped"} className="mb-3">
            <div className="px-2 pb-1 text-xs" style={{ opacity: 0.65 }}>{group || t("zdx.text.ssh.ungrouped")}</div>
            {items.map((host) => {
              const selected = host.id === selectedId;
              return (
                <div
                  key={host.id}
                  role="option"
                  aria-selected={selected}
                  tabIndex={0}
                  className="mb-1 flex cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 outline-none"
                  style={{ background: selected ? "rgba(79, 93, 240, 0.12)" : undefined }}
                  onClick={() => onSelect(host.id)}
                  onDoubleClick={() => onConnect(host)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") onConnect(host);
                  }}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{host.name}</div>
                    <div className="truncate text-xs" style={{ opacity: 0.65 }}>{host.username}@{host.host}:{host.port}</div>
                  </div>
                  <Tooltip title={t("zdx.label.ssh.connect")}>
                    <Button type="text" size="small" aria-label={t("zdx.label.ssh.connect")} icon={<CloudServerOutlined />} onClick={(event) => { event.stopPropagation(); onConnect(host); }} />
                  </Tooltip>
                  <Tooltip title={t("zdx.label.ssh.editHost")}>
                    <Button type="text" size="small" aria-label={t("zdx.label.ssh.editHost")} icon={<EditOutlined />} onClick={(event) => { event.stopPropagation(); onEdit(host); }} />
                  </Tooltip>
                  <Popconfirm
                    title={t("zdx.text.ssh.deleteConfirm", { name: host.name })}
                    okText={t("zdx.label.ssh.deleteHost")}
                    cancelText={t("zdx.label.ssh.cancel")}
                    okButtonProps={{ danger: true }}
                    onConfirm={() => onDelete(host)}
                  >
                    <Button type="text" size="small" danger aria-label={t("zdx.label.ssh.deleteHost")} icon={<DeleteOutlined />} onClick={(event) => event.stopPropagation()} />
                  </Popconfirm>
                </div>
              );
            })}
          </section>
        ))}
      </div>
    </div>
  );
}
